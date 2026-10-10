#include "include/drivetest.hpp"
#include <iostream>
#include <vector>
#include <cstring>
#include <chrono>
#include <cmath>
#include <windows.h>

using namespace std;

CONSOLE_SCREEN_BUFFER_INFO csbi;

extern HANDLE hConsole;
extern bool isLangRu;
extern int defCol, errCol, selCol;

// Optimal block size and chunk size for high throughput, direct I/O and FAT32 compliance
const size_t BLOCK_SIZE = 1024 * 1024;                               // 1 MB block size
const unsigned long long CHUNK_SIZE = 1024ULL * 1024ULL * 1024ULL;   // 1 GB per chunk (< 4 GB FAT32 limit)
const int OK_DISPLAY_INTERVAL = 4;                                   // Display every 4-th OK block (4 MB)

// Write and Read latency thresholds (ms) per 1 MB block
const int OK_THRESHOLD = 75;
const int VERY_SLOW_THRESHOLD = 250;
const int CRIT_THRESHOLD = 2000;
const int FAKE_CAPACITY_BAD_COUNT = 30; // 30 consecutive failed blocks indicates fake capacity

/*
* @brief RAII wrapper for sector-aligned buffer allocated via VirtualAlloc
*/
struct AlignedBuffer {
    char* data = nullptr;
    AlignedBuffer(size_t size) {
        data = static_cast<char*>(VirtualAlloc(NULL, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
    }
    ~AlignedBuffer() {
        if (data) {
            VirtualFree(data, 0, MEM_RELEASE);
        }
    }
    char* get() { return data; }
    bool isValid() const { return data != nullptr; }
};

/*
* @brief Fills memory block with deterministic verifiable pattern
*/
inline void fillBlockPattern(char* buffer, size_t size, size_t blockIndex) {
    uint64_t* p = reinterpret_cast<uint64_t*>(buffer);
    size_t words = size / sizeof(uint64_t);
    uint64_t base = static_cast<uint64_t>(blockIndex) << 32;
    for (size_t i = 0; i < words; ++i) {
        p[i] = base ^ static_cast<uint64_t>(i) ^ 0x5AA55AA5ULL;
    }
}

/*
* @brief Verifies memory block against deterministic pattern
*/
inline bool verifyBlockPattern(const char* buffer, size_t size, size_t blockIndex) {
    const uint64_t* p = reinterpret_cast<const uint64_t*>(buffer);
    size_t words = size / sizeof(uint64_t);
    uint64_t base = static_cast<uint64_t>(blockIndex) << 32;
    for (size_t i = 0; i < words; ++i) {
        if (p[i] != (base ^ static_cast<uint64_t>(i) ^ 0x5AA55AA5ULL)) {
            return false;
        }
    }
    return true;
}

/*
* @brief Generates path to chunk file (e.g. X:\usb-test.001)
*/
wstring getChunkFilePath(char driveLetter, size_t chunkIndex) {
    wchar_t filename[64];
    swprintf_s(filename, L"%c:\\usb-test.%03zu", static_cast<wchar_t>(towupper(driveLetter)), chunkIndex + 1);
    return wstring(filename);
}

/*
* @brief Opens chunk file with direct I/O (FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH)
*/
HANDLE openChunkForWriting(const wstring& chunkPath) {
    HANDLE hFile = CreateFileW(
        chunkPath.c_str(),
        GENERIC_WRITE,
        FILE_SHARE_READ,
        NULL,
        CREATE_ALWAYS,
        FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH,
        NULL
    );
    if (hFile == INVALID_HANDLE_VALUE) {
        hFile = CreateFileW(
            chunkPath.c_str(),
            GENERIC_WRITE,
            FILE_SHARE_READ,
            NULL,
            CREATE_ALWAYS,
            FILE_FLAG_WRITE_THROUGH,
            NULL
        );
    }
    return hFile;
}

/*
* @brief Opens chunk file for direct unbuffered reading
*/
HANDLE openChunkForReading(const wstring& chunkPath) {
    HANDLE hFile = CreateFileW(
        chunkPath.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ,
        NULL,
        OPEN_EXISTING,
        FILE_FLAG_NO_BUFFERING,
        NULL
    );
    if (hFile == INVALID_HANDLE_VALUE) {
        hFile = CreateFileW(
            chunkPath.c_str(),
            GENERIC_READ,
            FILE_SHARE_READ,
            NULL,
            OPEN_EXISTING,
            0,
            NULL
        );
    }
    return hFile;
}

/*
* @brief Safely removes test files from the target drive without destructive formatting
* @param char driveLetter: Target drive letter
*/
void cleanupTestFiles(char driveLetter) {
    wstring searchPattern = wstring(1, static_cast<wchar_t>(towupper(driveLetter))) + L":\\usb-test*";
    WIN32_FIND_DATAW findData;
    HANDLE hFind = FindFirstFileW(searchPattern.c_str(), &findData);
    if (hFind != INVALID_HANDLE_VALUE) {
        do {
            wstring filePath = wstring(1, static_cast<wchar_t>(towupper(driveLetter))) + L":\\" + findData.cFileName;
            DeleteFileW(filePath.c_str());
        } while (FindNextFileW(hFind, &findData));
        FindClose(hFind);
    }
}

/*
* @brief Write test part with block I/O and FAT32 chunk segmentation
*
* @param
* int selectedDrive: Selected drive index in drives[];
* std::vector<char> drives: Array with drive letters;
* int selectedTest: Selected test number (1 - write, 2 - write and read)
*
* @return Returns true if test completed successfully or false if not
*/
bool driveTestWritePart(int selectedDrive, std::vector<char>& drives, int selectedTest) {
    GetConsoleScreenBufferInfo(hConsole, &csbi);
    int consoleWidth = csbi.srWindow.Right - csbi.srWindow.Left + 1;

    int slowCount = 0, verySlowCount = 0, critCount = 0, badCount = 0;
    unsigned long long okCount = 0;
    int badSectorsRowCount = 0;
    size_t successfulBlocks = 0;

    ULARGE_INTEGER availableBytes = getDrivesInfo(drives, selectedDrive, false);
    unsigned long long availableMegabytes = availableBytes.QuadPart / (1024ULL * 1024ULL);

    size_t totalBlocks = static_cast<size_t>(availableBytes.QuadPart / BLOCK_SIZE);
    if (totalBlocks == 0) {
        SetConsoleTextAttribute(hConsole, errCol);
        (isLangRu) ? cerr << "Недостаточно свободного места на накопителе для проведения теста.\n"
                   : cerr << "Not enough free space on drive to run testing.\n";
        SetConsoleTextAttribute(hConsole, defCol);
        system("pause");
        return false;
    }

    size_t blocksPerChunk = static_cast<size_t>(CHUNK_SIZE / BLOCK_SIZE);
    size_t totalChunks = (totalBlocks + blocksPerChunk - 1) / blocksPerChunk;

    msgBeforeTest();
    system("pause");
    system("cls");
    msgBeforeTest();

    cleanupTestFiles(drives[selectedDrive]);

    AlignedBuffer buffer(BLOCK_SIZE);
    if (!buffer.isValid()) {
        SetConsoleTextAttribute(hConsole, errCol);
        (isLangRu) ? cerr << "Ошибка выделения памяти для буфера ввода-вывода.\n"
                   : cerr << "Failed to allocate memory for I/O buffer.\n";
        SetConsoleTextAttribute(hConsole, defCol);
        system("pause");
        return false;
    }

    auto startWrite = chrono::high_resolution_clock::now();
    size_t globalBlock = 0;
    int currentChar = 0;
    int displayCounter = 0;

    for (size_t chunkIdx = 0; chunkIdx < totalChunks; ++chunkIdx) {
        wstring chunkPath = getChunkFilePath(drives[selectedDrive], chunkIdx);
        HANDLE hFile = openChunkForWriting(chunkPath);
        if (hFile == INVALID_HANDLE_VALUE) {
            SetConsoleTextAttribute(hConsole, errCol);
            (isLangRu) ? cerr << "Ошибка при создании файла фрагмента теста.\n"
                       : cerr << "Failed to create chunk file for testing.\n";
            SetConsoleTextAttribute(hConsole, defCol);
            cleanupTestFiles(drives[selectedDrive]);
            return false;
        }

        size_t blocksInThisChunk = (chunkIdx == totalChunks - 1)
            ? (totalBlocks - chunkIdx * blocksPerChunk)
            : blocksPerChunk;

        for (size_t b = 0; b < blocksInThisChunk; ++b, ++globalBlock) {
            fillBlockPattern(buffer.get(), BLOCK_SIZE, globalBlock);

            DWORD bytesWritten = 0;
            auto start = chrono::high_resolution_clock::now();
            BOOL success = WriteFile(hFile, buffer.get(), static_cast<DWORD>(BLOCK_SIZE), &bytesWritten, NULL);
            auto end = chrono::high_resolution_clock::now();

            auto writeDuration = chrono::duration_cast<chrono::milliseconds>(end - start).count();

            if (!success || bytesWritten != static_cast<DWORD>(BLOCK_SIZE)) {
                SetConsoleTextAttribute(hConsole, (15 << 4) | 0);
                cout << "#";
                currentChar++;
                badCount++;
                badSectorsRowCount++;

                if (badSectorsRowCount > FAKE_CAPACITY_BAD_COUNT) {
                    CloseHandle(hFile);
                    cleanupTestFiles(drives[selectedDrive]);
                    unsigned long long realCapacity = (successfulBlocks * BLOCK_SIZE) / (1024ULL * 1024ULL);
                    msgFakeCapacity(realCapacity);
                    return false;
                }
            } else {
                successfulBlocks++;
                badSectorsRowCount = 0;

                if (writeDuration > CRIT_THRESHOLD) {
                    SetConsoleTextAttribute(hConsole, (4 << 4) | 4);
                    cout << "#";
                    critCount++;
                    currentChar++;
                } else if (writeDuration > VERY_SLOW_THRESHOLD) {
                    SetConsoleTextAttribute(hConsole, (6 << 4) | 6);
                    cout << "#";
                    verySlowCount++;
                    currentChar++;
                } else if (writeDuration > OK_THRESHOLD) {
                    SetConsoleTextAttribute(hConsole, (8 << 4) | 8);
                    cout << "#";
                    slowCount++;
                    currentChar++;
                } else {
                    okCount++;
                    displayCounter++;
                    if (displayCounter >= OK_DISPLAY_INTERVAL) {
                        SetConsoleTextAttribute(hConsole, (15 << 4) | 15);
                        cout << "#";
                        currentChar++;
                        displayCounter = 0;
                    }
                }
            }

            if (currentChar >= consoleWidth) {
                SetConsoleTextAttribute(hConsole, defCol);
                cout << "\n";
                currentChar = 0;
            }
        }

        CloseHandle(hFile);
    }

    auto endWrite = chrono::high_resolution_clock::now();
    auto writeTime = (chrono::duration_cast<chrono::milliseconds>(endWrite - startWrite).count()) / 1000.0;
    if (writeTime < 0.001) { writeTime = 0.001; }
    double totalMegabytesTested = static_cast<double>(successfulBlocks * BLOCK_SIZE) / (1024.0 * 1024.0);
    double writeSpeed = std::round((totalMegabytesTested / writeTime) * 10.0) / 10.0;

    SetConsoleTextAttribute(hConsole, (0 << 4) | 10);
    (isLangRu) ? cout << "\nТест записи завершен.\n\n" : cout << "\nWrite test finished.\n\n";
    SetConsoleTextAttribute(hConsole, defCol);

    if (selectedTest == 2) {
        driveTestReadPart(selectedDrive, drives, okCount, slowCount, verySlowCount, critCount, badCount, writeSpeed, availableMegabytes);
    } else {
        int fbDialogAnswer = displayFeedbackDialog();
        msgWriteTestResults(okCount, slowCount, verySlowCount, critCount, badCount, writeSpeed);

        SetConsoleTextAttribute(hConsole, errCol);
        (isLangRu) ? cout << "\nДля сохранения результатов в историю проведите Write&Read тест \n"
                   : cout << "Run a Write&Read test to save results to history. \n";
        SetConsoleTextAttribute(hConsole, defCol);

        msgFormatBeforeExiting();
        system("pause");

        collectFeedback(fbDialogAnswer);

        cleanupTestFiles(drives[selectedDrive]);
    }

    return true;
}

/*
* @brief Read test part with unbuffered direct block I/O
*/
bool driveTestReadPart(int selectedDrive, std::vector<char>& drives, unsigned long long okCountWrite, int slowCountWrite, int verySlowCountWrite, int critCountWrite, int badCountWrite, double writeSpeed, unsigned long long availableMegabytes) {
    GetConsoleScreenBufferInfo(hConsole, &csbi);
    int consoleWidth = csbi.srWindow.Right - csbi.srWindow.Left + 1;

    int critCount = 0, badCount = 0, slowCount = 0, verySlowCount = 0;
    unsigned long long okCount = 0;
    size_t successfulBlocksRead = 0;

    ULARGE_INTEGER availableBytes = getDrivesInfo(drives, selectedDrive, false);
    size_t totalBlocks = static_cast<size_t>(availableBytes.QuadPart / BLOCK_SIZE);
    if (totalBlocks == 0) {
        cleanupTestFiles(drives[selectedDrive]);
        return false;
    }

    size_t blocksPerChunk = static_cast<size_t>(CHUNK_SIZE / BLOCK_SIZE);
    size_t totalChunks = (totalBlocks + blocksPerChunk - 1) / blocksPerChunk;

    AlignedBuffer buffer(BLOCK_SIZE);
    if (!buffer.isValid()) {
        SetConsoleTextAttribute(hConsole, errCol);
        (isLangRu) ? cerr << "Ошибка выделения памяти для буфера чтения.\n"
                   : cerr << "Failed to allocate memory for read buffer.\n";
        SetConsoleTextAttribute(hConsole, defCol);
        cleanupTestFiles(drives[selectedDrive]);
        return false;
    }

    auto startRead = chrono::high_resolution_clock::now();
    size_t globalBlock = 0;
    int currentChar = 0;
    int displayCounter = 0;

    for (size_t chunkIdx = 0; chunkIdx < totalChunks; ++chunkIdx) {
        wstring chunkPath = getChunkFilePath(drives[selectedDrive], chunkIdx);
        HANDLE hFile = openChunkForReading(chunkPath);
        if (hFile == INVALID_HANDLE_VALUE) {
            SetConsoleTextAttribute(hConsole, errCol);
            (isLangRu) ? cerr << "Ошибка при открытии фрагмента файла для проверки.\n"
                       : cerr << "Unable to open chunk file for testing.\n";
            SetConsoleTextAttribute(hConsole, defCol);
            cleanupTestFiles(drives[selectedDrive]);
            return false;
        }

        size_t blocksInThisChunk = (chunkIdx == totalChunks - 1)
            ? (totalBlocks - chunkIdx * blocksPerChunk)
            : blocksPerChunk;

        for (size_t b = 0; b < blocksInThisChunk; ++b, ++globalBlock) {
            DWORD bytesRead = 0;
            auto start = chrono::high_resolution_clock::now();
            BOOL success = ReadFile(hFile, buffer.get(), static_cast<DWORD>(BLOCK_SIZE), &bytesRead, NULL);
            auto end = chrono::high_resolution_clock::now();

            auto readDuration = chrono::duration_cast<chrono::milliseconds>(end - start).count();

            bool valid = success && (bytesRead == static_cast<DWORD>(BLOCK_SIZE)) && verifyBlockPattern(buffer.get(), BLOCK_SIZE, globalBlock);

            if (!valid) {
                SetConsoleTextAttribute(hConsole, (15 << 4) | 0);
                cout << "#";
                currentChar++;
                badCount++;
            } else {
                successfulBlocksRead++;

                if (readDuration > CRIT_THRESHOLD) {
                    SetConsoleTextAttribute(hConsole, (4 << 4) | 4);
                    cout << "#";
                    critCount++;
                    currentChar++;
                } else if (readDuration > VERY_SLOW_THRESHOLD) {
                    SetConsoleTextAttribute(hConsole, (6 << 4) | 6);
                    cout << "#";
                    verySlowCount++;
                    currentChar++;
                } else if (readDuration > OK_THRESHOLD) {
                    SetConsoleTextAttribute(hConsole, (8 << 4) | 8);
                    cout << "#";
                    slowCount++;
                    currentChar++;
                } else {
                    okCount++;
                    displayCounter++;
                    if (displayCounter >= OK_DISPLAY_INTERVAL) {
                        SetConsoleTextAttribute(hConsole, (15 << 4) | 15);
                        cout << "#";
                        currentChar++;
                        displayCounter = 0;
                    }
                }
            }

            if (currentChar >= consoleWidth) {
                SetConsoleTextAttribute(hConsole, defCol);
                cout << "\n";
                currentChar = 0;
            }
        }

        CloseHandle(hFile);
    }

    auto endRead = chrono::high_resolution_clock::now();
    auto readTime = (chrono::duration_cast<chrono::milliseconds>(endRead - startRead).count()) / 1000.0;
    if (readTime < 0.001) { readTime = 0.001; }
    double totalMegabytesRead = static_cast<double>(successfulBlocksRead * BLOCK_SIZE) / (1024.0 * 1024.0);
    double readSpeed = std::round((totalMegabytesRead / readTime) * 10.0) / 10.0;

    SetConsoleTextAttribute(hConsole, 7);

    string driveLetter(1, drives[selectedDrive]);
    pushBackData(driveLetter, getTotalMegabytes(drives, selectedDrive), slowCountWrite, verySlowCountWrite, critCountWrite, critCount, badCountWrite, badCount, writeSpeed, readSpeed);

    SetConsoleTextAttribute(hConsole, 10);
    (isLangRu) ? cout << "\nТест чтения завершен.\n\n\n\n" : cout << "\nRead test finished.\n\n\n\n";
    SetConsoleTextAttribute(hConsole, defCol);

    int fbDialogAnswer = displayFeedbackDialog();

    msgReadTestResults(okCountWrite, slowCountWrite, verySlowCountWrite, critCountWrite, badCountWrite, critCount, badCount, readSpeed, writeSpeed);

    if (isLangRu) {
        cout << "\n\nРезультаты теста сохранены в: ";
        SetConsoleTextAttribute(hConsole, errCol);
        cout << getDataPath() << endl;
    } else {
        cout << "\n\nTest results saved to: ";
        SetConsoleTextAttribute(hConsole, errCol);
        cout << getDataPath() << endl;
    }
    SetConsoleTextAttribute(hConsole, defCol);

    msgFormatBeforeExiting();
    system("pause");

    collectFeedback(fbDialogAnswer);

    cleanupTestFiles(drives[selectedDrive]);

    return true;
}