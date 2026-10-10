# Аудит безопасности и дефектов кодовой базы (C++)

В данном документе зафиксирован полный реестр уязвимостей, дефектов надежности и архитектурных проблем, выявленных в ходе аудита C++ версии утилиты **USB Test** (ветка `fix` / `main`).

---

## 1. Сводная таблица инцидентов

| ID | Уязвимость / Дефект | Модуль | CWE / Класс | Критичность |
|---|---|---|---|:---:|
| SEC-01 | Отсутствие фильтрации съемных дисков и риск форматирования системного раздела | [driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp#L120-L140) | CWE-284, CWE-862 | Critical |
| SEC-02 | Принудительное отключение валидации SSL/TLS сертификатов | [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp#L125-L135) | CWE-295 | Critical |
| SEC-03 | Передача телеметрии по открытому HTTP и игнорирование отказа пользователя | [feedback.cpp](file:///d:/Projects/usb-test/USB%20Test/src/feedback.cpp#L20-L140) | CWE-319, CWE-359 | High |
| SEC-04 | Выход за границы вектора `drives` при вводе некорректного номера диска | [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp#L20-L36) | CWE-129, CWE-125 | High |
| ENG-01 | Искажение замеров скорости и задержек системным дисковым кэшем RAM | [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L65-L85) | Architectural Flaw | High |
| ENG-02 | Деградация скорости и износ накопителя при посекторной записи (512 байт) | [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L70-L100) | Performance / Design | High |
| ENG-03 | Ложное срабатывание детектора подделок из-за залипания флага `failbit` | [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L90-L115) | Logic Error / Bug | High |
| ENG-04 | Падение теста на файловых системах FAT32 при объеме более 4 ГБ | [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L50-L65) | Compatibility Flaw | Medium |
| STB-01 | Исчерпание стека (Stack Overflow) из-за взаимной рекурсии меню и `cin.fail()` | [menu.cpp](file:///d:/Projects/usb-test/USB%20Test/src/menu.cpp#L40-L120) | CWE-674 | High |
| STB-02 | Аварийное завершение при повреждении JSON или невалидном вводе | [json_utils.cpp](file:///d:/Projects/usb-test/USB%20Test/src/json_utils.cpp#L20-L40), [history.cpp](file:///d:/Projects/usb-test/USB%20Test/src/history.cpp#L100-L130) | CWE-754, CWE-20 | Medium |
| STB-03 | Накопление `drivesCount` и чтение за границами `drivesRaw[500]` | [driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp#L25-L60) | CWE-125 | Medium |
| STB-04 | Статическая инициализация Win32 Shell API до входа в `main()` | [main.cpp](file:///d:/Projects/usb-test/USB%20Test/src/main.cpp#L14), [json_utils.cpp](file:///d:/Projects/usb-test/USB%20Test/src/json_utils.cpp#L11) | CWE-665 | Medium |
| QAL-01 | Игнорирование patch-версий в модуле проверки обновлений | [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp#L65-L75) | Logic Error | Low |
| QAL-02 | Рассинхронизация версий и неиспользуемый файл `.cpp` в репозитории | [USB Test/.cpp](file:///d:/Projects/usb-test/USB%20Test/.cpp), [resource.h](file:///d:/Projects/usb-test/USB%20Test/src/resources/resource.h) | Repository Hygiene | Low |

---

## 2. Детальное описание инцидентов

### SEC-01: Отсутствие фильтрации съемных дисков и риск форматирования системного раздела
* **Файлы:** [driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp#L25-L140), [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp#L130-L210)
* **Класс:** CWE-284 (Improper Access Control), CWE-862 (Missing Authorization)
* **Критичность:** Critical
* **Описание:**
  Функция `getDrives()` вызывает `GetLogicalDriveStringsA` и помещает в вектор `drives` абсолютно все логические диски без валидации типа устройства через `GetDriveTypeA`. В меню выбора отображаются системный раздел `C:`, встроенные физические диски, сетевые и виртуальные тома.
  В функции `formatDisk()` выполняется принудительное форматирование выбранной буквы диска:
  ```cpp
  format = "format " + std::string(1, drives[selectedDrive]) + ": /q /x /y /FS:NTFS >NUL 2>&1";
  system(format.c_str());
  ```
* **Последствия:** При ошибочном выборе номера системного или вторичного локального накопителя происходит попытка его форматирования. Дополнительно команда переводит файловую систему в NTFS, что нарушает совместимость флеш-накопителей с автомобильными медиасистемами, ТВ и UEFI.
* **Рекомендация:**
  1. Разрешить сканирование только томов с `GetDriveTypeW(root) == DRIVE_REMOVABLE`.
  2. Запретить выбор системного тома через сравнение буквы с результатом `GetSystemDirectoryW`.
  3. Отказаться от форматирования всего накопителя через `system("format ...")` в пользу создания и последующего удаления изолированного файла теста (`DeleteFileW`).

---

### SEC-02: Принудительное отключение валидации SSL/TLS сертификатов
* **Файл:** [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp#L125-L135)
* **Класс:** CWE-295 (Improper Certificate Validation)
* **Критичность:** Critical
* **Описание:**
  В функции `checkUpdates()` для WinHTTP установлены флаги игнорирования всех ошибок сертификата:
  ```cpp
  DWORD flags = SECURITY_FLAG_IGNORE_UNKNOWN_CA |
                SECURITY_FLAG_IGNORE_CERT_CN_INVALID |
                SECURITY_FLAG_IGNORE_CERT_DATE_INVALID;
  WinHttpSetOption(hRequest, WINHTTP_OPTION_SECURITY_FLAGS, &flags, sizeof(flags));
  ```
* **Последствия:** Приложение уязвимо к Man-in-the-Middle (MitM) атакам. Любой узел в сети передачи данных может подменить целевой хост `4746220.atspace.cc` самоподписанным сертификатом и вернуть произвольные ответы (включая фишинговые ссылки на скачивание вредоносных бинарников).
* **Рекомендация:** Удалить отключение флагов безопасности. Использовать стандартную проверку доверенных корневых центров сертификации Windows.

---

### SEC-03: Передача телеметрии по открытому HTTP и игнорирование отказа пользователя
* **Файл:** [feedback.cpp](file:///d:/Projects/usb-test/USB%20Test/src/feedback.cpp#L20-L140)
* **Класс:** CWE-319 (Cleartext Transmission of Sensitive Information), CWE-359 (Privacy Violation)
* **Критичность:** High
* **Описание:**
  1. Сетевое соединение открывается по незащищенному протоколу HTTP на порт 80 бесплатного хостинга:
     ```cpp
     WinHttpConnect(hSession, L"4746220.atspace.cc", INTERNET_DEFAULT_HTTP_PORT, 0);
     ```
  2. Если пользователь в диалоге нажимает кнопку **«Нет»** (`IDNO`), программа игнорирует решение пользователя и всё равно отправляет сетевой маяк:
     ```cpp
     else if (dialogAnswer == IDNO) {
         string feedback = "DIDN'T ANSWER";
         sendFeedback(feedback);
     }
     ```
  3. Поле `feedback` конкатенируется напрямую без экранирования спецсимволов URL (`&`, `=`, `%`), что позволяет искажать структуру POST-запроса.
* **Рекомендация:**
  1. Полностью блокировать вызов `sendFeedback()` при выборе `IDNO`.
  2. Перевести передачу на HTTPS.
  3. Реализовать корректный URL-encoding передаваемых параметров.

---

### SEC-04: Выход за границы вектора `drives` при некорректном выборе диска
* **Файл:** [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp#L20-L36)
* **Класс:** CWE-129 (Improper Validation of Array Index), CWE-125 (Out-of-bounds Read)
* **Критичность:** High
* **Описание:**
  В функции `msgSelectDrive()` валидируется только верхняя граница:
  ```cpp
  cin >> selectedDrive;
  if (selectedDrive > drivesCount || cin.fail()) {
      msgInputError(true);
  }
  selectedDrive--;
  return selectedDrive;
  ```
  Если пользователь вводит `0` или отрицательное число, условие не срабатывает. После декремента переменная получает отрицательный индекс (`-1`, `-2`).
* **Последствия:** При вызове `drives[selectedDrive]` происходит чтение за пределами диапазона `std::vector`, что вызывает аварийный сбой процесса (Crash / SIGSEGV).
* **Рекомендация:** Добавить строгую проверку диапазона:
  ```cpp
  if (cin.fail() || selectedDrive < 1 || selectedDrive > drivesCount)
  ```

---

### ENG-01: Искажение результатов тестов системным дисковым кэшем RAM
* **Файл:** [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L65-L85), [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L185-L210)
* **Класс:** Архитектурный дефект I/O
* **Критичность:** High
* **Описание:**
  Операции записи и чтения выполняются через стандартные потоки `std::ofstream`/`std::ifstream` без обхода кэша ОС:
  - Отсутствуют флаги прямого доступа `FILE_FLAG_NO_BUFFERING` и `FILE_FLAG_WRITE_THROUGH`.
  - Вызов `file.flush()` освобождает только внутренний буфер C++ рантайма, не вызывая аппаратный сброс буферов накопителя через `FlushFileBuffers`.
  - В тесте чтения файл открывается сразу же после завершения записи, считывая горячие страницы непосредственно из оперативной памяти Windows.
* **Последствия:** Программа измеряет пропускную способность RAM и системного дискового кэша Windows, а не реальные характеристики и деградацию физических ячеек NAND Flash.
* **Рекомендация:** Использовать Win32 API `CreateFileW` с флагами прямого блочного ввода-вывода (`FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH`) с обязательным секторным выравниванием буферов памяти.

---

### ENG-02: Катастрофическая деградация скорости при посекторной записи (512 байт)
* **Файл:** [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L70-L100)
* **Класс:** Архитектурный дефект производительности
* **Критичность:** High
* **Описание:**
  Тест производит запись микро-блоками по 512 байт с замером таймера `chrono::high_resolution_clock` и сбросом `file.flush()` на каждой итерации.
* **Последствия:**
  - Для накопителя объемом 64 ГБ генерируется свыше 134 миллионов системных вызовов.
  - Накладные расходы контекстных переключений операционной системы и высокий Write Amplification контроллера флешки снижают эффективную скорость записи до критически низких значений. Полное тестирование может занимать десятки часов и вызывать ускоренный износ ячеек.
* **Рекомендация:** Осуществлять блочный ввод-вывод оптимальными пакетами от 256 КБ до 1 МБ с расчетом поблочной латентности.

---

### ENG-03: Ложное срабатывание детектора подделок из-за залипания флага `failbit`
* **Файл:** [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L90-L115)
* **Класс:** Ошибка логики обработки ошибок I/O
* **Критичность:** High
* **Описание:**
  При первой же аппаратной ошибке или задержке записи поток `ofstream` переходит в состояние ошибки (`failbit`).
  ```cpp
  if (!file) {
      badCount++;
      badSectorsRowCount++;
  }
  ```
  Метод `file.clear()` не вызывается. Согласно стандарту C++, все последующие операции записи над данным потоком игнорируются.
* **Последствия:** Счетчик сбойных секторов `badSectorsRowCount` лавинообразно достигает порога `FAKE_CAPACITY_BAD_COUNT (1000)`, в результате чего исправный накопитель ошибочно бракуется как поддельный.
* **Рекомендация:** Обрабатывать статус каждой конкретной операции и сбрасывать состояние ошибки потока или перейти на нативные дескрипторы файлов с анализом кодов ошибок Win32.

---

### ENG-04: Падение теста на файловых системах FAT32 при объеме более 4 ГБ
* **Файл:** [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp#L50-L65)
* **Класс:** Ограничение файловой системы
* **Критичность:** Medium
* **Описание:**
  Тестовые данные пишутся в единый файл `X:\usb-test` на весь доступный объем накопителя. Накопители объемом 8–32 ГБ стандартно отформатированы в FAT32, где максимальный размер отдельного файла ограничен $4\text{ ГБ} - 1\text{ байт}$.
* **Последствия:** Запись неизбежно падает с ошибкой переполнения файловой системы при достижении отметки 4 ГБ.
* **Рекомендация:** Разделять тестовые файлы на фрагменты размером до 2 ГБ или работать напрямую с блочным устройством тома.

---

### STB-01: Исчерпание стека (Stack Overflow) из-за взаимной рекурсии меню и `cin.fail()`
* **Файлы:** [menu.cpp](file:///d:/Projects/usb-test/USB%20Test/src/menu.cpp), [history.cpp](file:///d:/Projects/usb-test/USB%20Test/src/history.cpp)
* **Класс:** CWE-674 (Uncontrolled Recursion)
* **Критичность:** High
* **Описание:**
  1. Навигация между окнами консоли построена на рекурсивных вызовах (`mainMenu` -> `menuHistory` -> `mainMenu`). Вызовы не возвращают управление, накапливая стековые кадры.
  2. При некорректном пользовательском вводе (например, ввод символов вместо цифр в `menuHistory`):
     ```cpp
     cin >> selectedOption;
     if (selectedOption > 5 || selectedOption < 1 || cin.fail()) {
         msgInputError(false);
         system("cls");
         menuHistory();
     }
     ```
     Функция `msgInputError(false)` не очищает флаг ошибки `cin` и не извлекает мусор из буфера (`cin.clear()` и `cin.ignore()` отсутствуют). Следующий вызов `menuHistory()` мгновенно падает на том же вводе и бесконечно вызывает сам себя.
* **Последствия:** Исчерпание стека и падение процесса с кодом исключения `0xC00000FD` (Stack Overflow).
* **Рекомендация:** Организовать логику через единый цикл обработки событий `while (running)` и очищать состояние входного потока при `cin.fail()`.

---

### STB-02: Аварийное завершение при повреждении JSON или невалидном вводе
* **Файлы:** [json_utils.cpp](file:///d:/Projects/usb-test/USB%20Test/src/json_utils.cpp#L20-L40), [history.cpp](file:///d:/Projects/usb-test/USB%20Test/src/history.cpp#L100-L130)
* **Класс:** CWE-754 (Improper Check for Unusual or Exceptional Conditions), CWE-20 (Improper Input Validation)
* **Критичность:** Medium
* **Описание:**
  - В `readData()` парсер `file >> jDrives` не защищен блоком `try...catch`. Любой синтаксический дефект в файле истории `UsbTestHistory.json` вызывает исключение `nlohmann::json::parse_error` и краш приложения.
  - В `importData()` обращение `jTestImport[0]["drive_letter"]` безусловно предполагает, что импортированный JSON является непустым массивом и содержит все ожидаемые поля. При импорте невалидного файла выбрасывается необработанное исключение.
  - В `compareTests()` вызов `std::stoi(optionOneValue)` не защищен от `std::invalid_argument`. В блоке ошибки открытия файла вызывается `mainMenu()`, но отсутствует `return`, после чего происходит попытка чтения из закрытого потока.
* **Рекомендация:** Обернуть операции парсинга JSON и конвертации строк в блоки `try...catch` с выводом сообщения об ошибке без завершения процесса.

---

### STB-03: Накопление `drivesCount` и чтение за границами буфера `drivesRaw`
* **Файл:** [driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp#L25-L60)
* **Класс:** CWE-125 (Out-of-bounds Read)
* **Критичность:** Medium
* **Описание:**
  - Глобальная переменная `int drivesCount = 0;` не сбрасывается в начале функции `getDrives()`. При повторных вызовах счетчик дисков суммируется.
  - При `i == 499` инкремент `++i` приводит к проверке `drivesRaw[500]`, выходящей за пределы выделенного массива `char drivesRaw[500]`.
  - Во втором цикле отсутствует проверка выхода индекса `i` за пределы массива при несовпадении `drivesCount`.
* **Рекомендация:** Сделать `drivesCount` локальной переменной или вектором, и выполнять безопасный парсинг null-terminated строк.

---

### STB-04: Статическая инициализация Win32 Shell API до входа в `main()`
* **Файлы:** [main.cpp](file:///d:/Projects/usb-test/USB%20Test/src/main.cpp#L14), [json_utils.cpp](file:///d:/Projects/usb-test/USB%20Test/src/json_utils.cpp#L11)
* **Класс:** CWE-665 (Improper Initialization)
* **Критичность:** Medium
* **Описание:**
  Глобальные переменные инициализируются до вызова `main()`:
  ```cpp
  std::vector<char> drives = getDrives();
  const string DATA_PATH = getDocumentsPath() + "\\" + "UsbTestHistory.json";
  ```
  Вызовы `GetLogicalDriveStringsA` и `SHGetFolderPathA` происходят до завершения инициализации рантайма и установки консольной кодировки. Это исключает динамическое обнаружение подключенных после старта флешек и может приводить к сбоям путей в не-ASCII профилях пользователей.
* **Рекомендация:** Перенести инициализацию списка дисков и путей конфигурации внутрь функции `main()`.

---

### QAL-01: Игнорирование patch-версий в модуле проверки обновлений
* **Файл:** [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp#L65-L75)
* **Класс:** Логическая ошибка
* **Критичность:** Low
* **Описание:**
  Функция сравнения версий проверяет только major и minor компоненты:
  ```cpp
  if (s1 != c1) return s1 > c1;
  return s2 > c2;
  ```
  Компонент `s3` (patch) игнорируется. Если на сервере доступна версия `1.1.6`, а клиент имеет версию `1.1.5`, обновление не будет обнаружено.
* **Рекомендация:** Реализовать корректное сравнение SemVer (сравнение `s3 > c3` при равенстве `s1 == c1` и `s2 == c2`).

---

### QAL-02: Рассинхронизация версий и неиспользуемый файл `.cpp` в репозитории
* **Файлы:** [USB Test/.cpp](file:///d:/Projects/usb-test/USB%20Test/.cpp), [resource.h](file:///d:/Projects/usb-test/USB%20Test/src/resources/resource.h)
* **Класс:** Нарушение чистоты репозитория
* **Критичность:** Low
* **Описание:**
  - В корне проекта находится файл `USB Test/.cpp` с невалидным подключением `#include "driveinfo.h"`.
  - В файле ресурсов `USB Test.rc` и в сообщении `msgAbout()` указана устаревшая версия `1.1.4`, в то время как в `version.cpp` используется `1.1.5`.
* **Рекомендация:** Удалить сиротский файл и синхронизировать номера версий в ресурсах.
