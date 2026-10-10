#include "include/driveinfo.hpp"
#include <iostream>
#include <windows.h>
#include <vector>

using namespace std;

extern int defCol, errCol, selCol;		//Console color codes
extern bool isLangRu;

extern HANDLE hConsole;

DISK_GEOMETRY dg;


/*
* @brief Gets all drive letters of removable drives (excluding system drive)
* @return Returns array of chars with drive letters ({'E', 'F'}), std::vector<char>
*/
std::vector<char> getDrives() {
	std::vector<char> drivesProcessed;

	DWORD bufferLength = GetLogicalDriveStringsW(0, NULL);
	if (bufferLength == 0) {
		return drivesProcessed;
	}

	std::vector<wchar_t> buffer(bufferLength + 1, 0);
	if (GetLogicalDriveStringsW(bufferLength, buffer.data()) == 0) {
		return drivesProcessed;
	}

	wchar_t systemDir[MAX_PATH] = { 0 };
	wchar_t systemDriveLetter = L'\0';
	if (GetSystemDirectoryW(systemDir, MAX_PATH) > 0) {
		systemDriveLetter = static_cast<wchar_t>(towupper(systemDir[0]));
	}

	const wchar_t* pDrive = buffer.data();
	while (*pDrive) {
		wchar_t driveLetterW = static_cast<wchar_t>(towupper(pDrive[0]));

		// Only include removable drives and exclude the system drive
		if (driveLetterW != systemDriveLetter && GetDriveTypeW(pDrive) == DRIVE_REMOVABLE) {
			drivesProcessed.push_back(static_cast<char>(driveLetterW));
		}

		pDrive += wcslen(pDrive) + 1;
	}

	return drivesProcessed;
}


/*
* @brief Returns information about available bytes on disc or displays table with information about drives.
*
* @param
* std::vector<char> drives: array with drive letters in it;
*
* int selectedDrive: selected drives number in drives[];
*
* bool isTableNeeded:
* When the program starts, an information table with all the data this function
* searches for is displayed on the screen. If displaying the table is not necessary,
* but rather just information about available bytes on disk is required, isTableNeeded should be False.
*
* @return Returns available bytes count on selected drive if selectedDrive isn't NULL (isn't provided yet)
*/
ULARGE_INTEGER getDrivesInfo(std::vector<char>& drives, int selectedDrive, bool isTableNeeded)
{
	ULARGE_INTEGER availableBytes = { 0 }, totalBytes = { 0 };

	SetConsoleTextAttribute(hConsole, defCol);

	if (selectedDrive >= 0 && selectedDrive < static_cast<int>(drives.size())) {		//If selected drive is provided and valid
		wchar_t driveLetter = drives[selectedDrive];		//Forming drive path
		wchar_t drivePath[4] = { driveLetter, L':' ,L'\\', L'\0' };
		GetDiskFreeSpaceExW(drivePath, &availableBytes, &totalBytes, nullptr);		//Getting information about drive capacity
	}

	if (isTableNeeded) {		//Displaying table if needed

		(isLangRu) ? cout << "Номер\tДиск\t\tСвободное место\t\tРазмер диска" << endl : cout << "No.\tDrive\t\tFree space\t\tDisk capacity" << endl;

		if (drives.empty()) {
			SetConsoleTextAttribute(hConsole, errCol);
			(isLangRu) ? cout << "Съёмные накопители не обнаружены." << endl : cout << "No removable drives detected." << endl;
			SetConsoleTextAttribute(hConsole, defCol);
		}
		else {
			for (size_t i = 0; i < drives.size(); i++)	//Displaying table line-by-line
			{
				wchar_t driveLetter = drives[i];
				wchar_t drivePath[4] = { driveLetter, L':' ,L'\\', L'\0' };
				GetDiskFreeSpaceExW(drivePath, &availableBytes, &totalBytes, nullptr);

				if (selectedDrive == static_cast<int>(i) && selectedDrive != -1) { SetConsoleTextAttribute(hConsole, selCol); }		//Looking for a selected drive (if != -1) to color it's line
				cout << i + 1 << "\t" << drives[i] << "\t\t" << availableBytes.QuadPart / (1024ULL * 1024ULL) << " MB   \t\t";
				if (availableBytes.QuadPart < 100ULL * (1024ULL * 1024ULL)) { cout << "\t"; }		//If capacity is lower than three symbols adds one tabulation
				cout << totalBytes.QuadPart / (1024ULL * 1024ULL) << " MB" << endl;

				SetConsoleTextAttribute(hConsole, defCol);
			}
		}
		cout << endl << endl;

	}

	if (selectedDrive >= 0 && selectedDrive < static_cast<int>(drives.size())) { return availableBytes; }
	else { return { 0 }; }
}



/*
* @brief Calculates how many bytes per sector drive has
*
* @param
* int selectedDrive: selected drives number in drives[];
*
* std::vector<char> drives: array with drive letters in it;
*
* @return Returns DWORD bd.bytesPerSector
*/
DWORD getSectorSize(int selectedDrive, std::vector<char>& drives) {

	DWORD bytesReturned;

	wstring devicePath = L"\\\\.\\";       //Forms string with disk's path
	devicePath += drives[selectedDrive];
	devicePath += L":";

	HANDLE hDevice = CreateFile(	//Opening disk
		devicePath.c_str(),
		GENERIC_READ,
		FILE_SHARE_READ |
		FILE_SHARE_WRITE,
		NULL,
		OPEN_EXISTING,
		0,
		NULL
	);

	if (hDevice == INVALID_HANDLE_VALUE) {      //Unable to open drive
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cout << "Не удалось открыть диск. Попробуйте запустить программу от имени Администратора \n" : cout << "Unable to access drive. Try running USB Test as Administrator \n";
		system("pause"); exit(2);
	}


	if ((DeviceIoControl(		//Getting bytes per sector
		hDevice,
		IOCTL_DISK_GET_DRIVE_GEOMETRY,
		NULL, 0,
		&dg, sizeof(dg),
		&bytesReturned,
		NULL)
		) && (dg.BytesPerSector >= 512 && dg.BytesPerSector <= 4096)) {
	}
	else {
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cout << "Ошибка в получении размера сектора!" : cout << "Unable to determine sector size!";
		system("pause"); exit(3);
	}

	CloseHandle(hDevice);

	return dg.BytesPerSector;
}


/*
* @brief Calculates target drive's capacity in megabytes
*
* @param
* int selectedDrive: selected drives number in drives[];
*
* std::vector<char> drives: array with drive letters in it;
*
* @return Returns total drive capacity in megabytes
*/
unsigned long long getTotalMegabytes(std::vector<char>& drives, int selectedDrive) {

	ULARGE_INTEGER totalBytes;

	if (selectedDrive != -1) {		//If selected drive is provided
		wchar_t driveLetter = drives[selectedDrive];		//Forming drive path
		wchar_t drivePath[4] = { driveLetter, L':' ,L'\\', L'\0' };
		GetDiskFreeSpaceEx(drivePath, nullptr, &totalBytes, nullptr);		//Getting information about drive capacity

		unsigned long long totalMegabytes = (totalBytes.QuadPart / (1024ULL * 1024ULL));	//Convert bytes to megabytes

		return totalMegabytes;
	}

	return 0ULL;
}