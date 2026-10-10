# План устранения дефектов и уязвимостей (issues.md)

В данном документе сформулирован пошаговый план исправления уязвимостей безопасности, архитектурных недостатков и дефектов стабильности утилиты **USB Test**, зафиксированных в [issues.md](file:///d:/Projects/usb-test/issues.md).

План разделен на 10 компактных фаз. Каждая фаза затрагивает изолированный контекст (1–3 файла), обеспечивает компилируемость кодовой базы и предоставляет конкретный перечень проверок для ручного ревью diff'а.

---

## Статус выполнения

| Фаза | Название | Затронутые инциденты | Ключевые файлы | Статус |
|:---:|---|:---:|---|:---:|
| 1 | Репозиторная гигиена и SemVer-сравнение | `QAL-01`, `QAL-02` | `.cpp`, `resource.h`, `USB Test.rc`, `messages.cpp`, `version.cpp` | `[x] Завершено` |
| 2 | Сетевая безопасность и приватность телеметрии | `SEC-02`, `SEC-03` | `version.cpp`, `feedback.cpp`, `feedback.hpp` | `[x] Завершено` |
| 3 | Жизненный цикл рантайма и устранение статической инициализации | `STB-04` | `main.cpp`, `json_utils.cpp`, `json_utils.hpp`, `driveinfo.hpp` | `[ ] Не начато` |
| 4 | Навигация консоли и устранение взаимной рекурсии | `STB-01` | `menu.cpp`, `menu.hpp`, `messages.cpp`, `history.cpp` | `[ ] Не начато` |
| 5 | Безопасное перечисление и валидация выбора накопителей | `SEC-01` (ч. 1), `SEC-04`, `STB-03` | `driveinfo.cpp`, `driveinfo.hpp`, `messages.cpp` | `[ ] Не начато` |
| 6 | Исключение деструктивного форматирования накопителя | `SEC-01` (ч. 2) | `driveinfo.cpp`, `drivetest.cpp`, `messages.cpp` | `[ ] Не начато` |
| 7 | Отказоустойчивость парсинга JSON и истории | `STB-02` | `json_utils.cpp`, `history.cpp` | `[ ] Не начато` |
| 8 | Логика тестирования: сброс ошибок потоков и поддержка FAT32 | `ENG-03`, `ENG-04` | `drivetest.cpp`, `drivetest.hpp` | `[ ] Не начато` |
| 9 | Переработка I/O: блочный доступ и обход системного кэша RAM | `ENG-01`, `ENG-02` | `drivetest.cpp` | `[ ] Не начато` |
| 10 | Комплексная верификация и регрессионное тестирование | Полный регресс | Вся кодовая база | `[ ] Не начато` |

---

## Детализация фаз

### Фаза 1. Репозиторная гигиена и SemVer-сравнение `[x] Завершено`
* **Инциденты:** `QAL-01`, `QAL-02`
* **Файлы:**
  - `USB Test/.cpp` (удаление)
  - [resource.h](file:///d:/Projects/usb-test/USB%20Test/src/resources/resource.h)
  - [USB Test.rc](file:///d:/Projects/usb-test/USB%20Test/src/resources/USB%20Test.rc)
  - [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp)
  - [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp)
* **Задачи:**
  1. Удалить неиспользуемый файл [USB Test/.cpp](file:///d:/Projects/usb-test/USB%20Test/.cpp) из корня проекта.
  2. Синхронизировать номер версии до `1.1.5` в [USB Test.rc](file:///d:/Projects/usb-test/USB%20Test/src/resources/USB%20Test.rc) (`FILEVERSION`, `PRODUCTVERSION`, строковые поля) и в сообщении `msgAbout()` в [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp).
  3. В функции `isNewerVersion()` модуля [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp) исправить логику сравнения SemVer: учитывать компонент `patch` (`s3 > c3` при равенстве `s1 == c1` и `s2 == c2`).
* **Критерии приемки:**
  - Успешная сборка через MSBuild (`Release|x64`).
  - Файл `USB Test/.cpp` отсутствует в дереве файлов git.
  - Версия `1.1.5` отображается единообразно во всех свойствах бинарника и в окне About.
  - Юнит-тест/ручная проверка `isNewerVersion("1.1.6", "1.1.5")` возвращает `true`.

---

### Фаза 2. Сетевая безопасность и приватность телеметрии `[x] Завершено`
* **Инциденты:** `SEC-02`, `SEC-03`
* **Файлы:**
  - [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp)
  - [feedback.cpp](file:///d:/Projects/usb-test/USB%20Test/src/feedback.cpp)
  - [feedback.hpp](file:///d:/Projects/usb-test/USB%20Test/src/include/feedback.hpp)
* **Задачи:**
  1. В [version.cpp](file:///d:/Projects/usb-test/USB%20Test/src/version.cpp) удалить установку флагов `SECURITY_FLAG_IGNORE_*` через `WinHttpSetOption`, задействовав стандартную валидацию сертификатов Windows CA.
  2. В [feedback.cpp](file:///d:/Projects/usb-test/USB%20Test/src/feedback.cpp):
     - Сохранить отправку сигнала `"DIDN'T ANSWER"` при `dialogAnswer == IDNO` согласно авторскому дизайну телеметрии.
     - Перевести сетевое подключение на HTTPS (`INTERNET_DEFAULT_HTTPS_PORT`, флаг `WINHTTP_FLAG_SECURE`).
     - Реализовать безопасное URL-кодирование (percent-encoding) строки отзыва перед сборкой POST-payload.
     - Исправить предупреждение компилятора C4267 при вызове `WinHttpSendRequest` (приведение типов `size_t` к `DWORD`).
* **Критерии приемки:**
  - Проект компилируется без C4267 warnings в `feedback.cpp`.
  - При выборе «Нет» в окне обратной связи отправляется маяк `"DIDN'T ANSWER"` по защищенному HTTPS-каналу.
  - Сетевые вызовы происходят исключительно по HTTPS (порт 443) с валидацией цепочки сертификатов.

---

### Фаза 3. Жизненный цикл рантайма и устранение статической инициализации
* **Инциденты:** `STB-04`
* **Файлы:**
  - [main.cpp](file:///d:/Projects/usb-test/USB%20Test/src/main.cpp)
  - [json_utils.cpp](file:///d:/Projects/usb-test/USB%20Test/src/json_utils.cpp)
  - [json_utils.hpp](file:///d:/Projects/usb-test/USB%20Test/src/include/json_utils.hpp)
  - [driveinfo.hpp](file:///d:/Projects/usb-test/USB%20Test/src/include/driveinfo.hpp)
* **Задачи:**
  1. Удалить глобальную инициализацию `std::vector<char> drives = getDrives();` до `main()`. Список дисков должен запрашиваться явно при входе в контекст работы с накопителями.
  2. Заменить глобальную константу `DATA_PATH = getDocumentsPath() + ...` на функцию-аксессор `const std::string& getDataPath()`, инициализирующую путь динамически при первом обращении после инициализации рантайма.
  3. Обеспечить корректный порядок вызовов в [main.cpp](file:///d:/Projects/usb-test/USB%20Test/src/main.cpp): установка кодировок/языка -> инициализация путей -> запуск интерфейса.
* **Критерии приемки:**
  - Нулевое количество системных вызовов Shell API (`SHGetFolderPath`) и Drive API (`GetLogicalDriveStrings`) в секции CRT-инициализации до `main()`.
  - Корректная локализация путей пользователя с не-ASCII символами в имени профиля.

---

### Фаза 4. Навигация консоли и устранение взаимной рекурсии
* **Инциденты:** `STB-01`
* **Файлы:**
  - [menu.cpp](file:///d:/Projects/usb-test/USB%20Test/src/menu.cpp)
  - [menu.hpp](file:///d:/Projects/usb-test/USB%20Test/src/include/menu.hpp)
  - [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp)
  - [history.cpp](file:///d:/Projects/usb-test/USB%20Test/src/history.cpp)
* **Задачи:**
  1. Заменить циклы взаимной рекурсии (`mainMenu` -> `menuHistory` -> `mainMenu`) на итеративный цикл диспетчеризации (конечный автомат / цикл `while (isRunning)`).
  2. В `msgInputError()` и обработчиках невалидного ввода добавить сброс состояния потока и очистку буфера:
     ```cpp
     cin.clear();
     cin.ignore(std::numeric_limits<std::streamsize>::max(), '\n');
     ```
  3. Обеспечить естественный возврат управления (`return`) из подменю в главный цикл без накопления стековых кадров.
* **Критерии приемки:**
  - Ввод нечисловых символов (букв, спецсимволов) в любых пунктах меню не приводит к зацикливанию или падению `0xC00000FD` (Stack Overflow).
  - Навигация "Главное меню -> История -> Назад -> Главное меню" выполняется произвольное число раз при неизменной глубине стека вызовов.

---

### Фаза 5. Безопасное перечисление и валидация выбора накопителей
* **Инциденты:** `SEC-01` (часть 1), `SEC-04`, `STB-03`
* **Файлы:**
  - [driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp)
  - [driveinfo.hpp](file:///d:/Projects/usb-test/USB%20Test/src/include/driveinfo.hpp)
  - [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp)
* **Задачи:**
  1. В `getDrives()` ([driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp)):
     - Устранить глобальную переменную `drivesCount` и фиксированный массив `drivesRaw[500]`.
     - Задействовать безопасный парсинг буфера `GetLogicalDriveStringsW`.
     - Фильтровать диски: включать в список только накопители типа `GetDriveTypeW(rootPath) == DRIVE_REMOVABLE`.
     - Исключать системный диск (букву каталога `GetSystemDirectoryW`).
  2. В `msgSelectDrive()` ([messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp)):
     - Добавить строгую проверку диапазона: `cin.fail() || selectedDrive < 1 || selectedDrive > drivesCount`.
     - Гарантировать, что при выходе за границы пользователь не получает отрицательный индекс в `drives[]`.
* **Критерии приемки:**
  - В таблице выбора дисков отображаются исключительно съемные USB-накопители; системный диск `C:` и несъемные HDD/SSD скрыты.
  - Ввод `0`, отрицательных чисел или номеров больше количества дисков выводит сообщение об ошибке и не приводит к Out-of-bounds Read (crash).

---

### Фаза 6. Исключение деструктивного форматирования накопителя
* **Инциденты:** `SEC-01` (часть 2)
* **Файлы:**
  - [driveinfo.cpp](file:///d:/Projects/usb-test/USB%20Test/src/driveinfo.cpp)
  - [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp)
  - [messages.cpp](file:///d:/Projects/usb-test/USB%20Test/src/messages.cpp)
* **Задачи:**
  1. Полностью удалить опасную команду `system("format ... /FS:NTFS")` и вызовы `formatDisk()`.
  2. Реализовать безопасную процедуру очистки: удаление созданных тестовых файлов через Win32 `DeleteFileW` по завершении или прерывании теста.
  3. Удалить или скорректировать сообщения `msgFormatDisk()` и `msgFormatBeforeExiting()`, предупреждающие об уничтожении файловой системы и разделов.
* **Критерии приемки:**
  - После завершения или отмены теста исходная файловая система накопителя (FAT32/exFAT/NTFS) и существующие файлы пользователя не уничтожаются.
  - Временные тестовые файлы корректно удаляются с накопителя.

---

### Фаза 7. Отказоустойчивость парсинга JSON и истории
* **Инциденты:** `STB-02`
* **Файлы:**
  - [json_utils.cpp](file:///d:/Projects/usb-test/USB%20Test/src/json_utils.cpp)
  - [history.cpp](file:///d:/Projects/usb-test/USB%20Test/src/history.cpp)
* **Задачи:**
  1. В `readData()` обернуть десериализацию `file >> jDrives` в блок `try ... catch (const nlohmann::json::parse_error& e)`: при повреждении файла выводить сообщение и инициализировать пустой массив вместо падения процесса.
  2. В `importData()` добавить валидацию структуры импортируемого документа:
     - Проверка `jTestImport.is_array()` и `!jTestImport.empty()`.
     - Проверка наличия обязательных полей (`drive_letter`, `total_mb`, `write_speed` и т.д.) через `contains()` перед чтением.
  3. В `compareTests()`:
     - Обернуть `std::stoi(optionValue)` в блок `try ... catch (const std::exception& e)`.
     - Добавить `return` в блоке обработки ошибок открытия файла (исключить чтение из закрытого дескриптора).
* **Критерии приемки:**
  - Повреждение файла `UsbTestHistory.json` не приводит к падению программы.
  - Импорт пустого, поврежденного или невалидного JSON-файла корректно отклоняется с выводом ошибки в консоль.

---

### Фаза 8. Логика тестирования: сброс ошибок потоков и поддержка FAT32
* **Инциденты:** `ENG-03`, `ENG-04`
* **Файлы:**
  - [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp)
  - [drivetest.hpp](file:///d:/Projects/usb-test/USB%20Test/src/include/drivetest.hpp)
* **Задачи:**
  1. В алгоритмах записи и чтения добавить сброс флагов ошибок потока (`file.clear()`) при единичных ошибках I/O, исключив залипание `failbit` и ложное срабатывание детектора поддельного объема (`badSectorsRowCount > FAKE_CAPACITY_BAD_COUNT`).
  2. Реализовать сегментирование тестового файла: вместо одного гигантского файла `usb-test` создавать последовательность фрагментов (чанков) размером до 2–4 ГБ (например, `usb-test.001`, `usb-test.002`), что снимает лимит FAT32 в 4 ГБ.
* **Критерии приемки:**
  - Накопители FAT32 объемом более 4 ГБ успешно проходят тестирование без ошибки переполнения файловой системы.
  - Единичный сбойный сектор не приводит к лавинообразному признанию всех последующих секторов сбойными.

---

### Фаза 9. Переработка I/O: блочный доступ и обход системного кэша RAM
* **Инциденты:** `ENG-01`, `ENG-02`
* **Файлы:**
  - [drivetest.cpp](file:///d:/Projects/usb-test/USB%20Test/src/drivetest.cpp)
* **Задачи:**
  1. Заменить микрозапись по 512 байт на оптимальный блочный размер (например, 256 КБ – 1 МБ), многократно сократив накладные расходы контекстных переключений ОС и снизив Write Amplification контроллера флеш-памяти.
  2. Реализовать прямой блочный ввод-вывод:
     - Использовать Win32 API `CreateFileW` с флагами `FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH` (с секторным выравниванием буферов в памяти через `VirtualAlloc` / `_aligned_malloc`) или вызывать `FlushFileBuffers` для изолированных интервалов.
     - Обеспечить достоверность измерений реальной скорости физического накопителя без влияния RAM-кэша Windows.
* **Критерии приемки:**
  - Скорость тестирования накопителей объемом 16–64 ГБ возрастает до физического максимума шины USB / чипа NAND (вместо часов при 512-байтовой записи).
  - Замеры скорости чтения отражают физическую скорость шины, а не мгновенное чтение горячих страниц из оперативной памяти.

---

### Фаза 10. Комплексная верификация и регрессионное тестирование
* **Инциденты:** Все инциденты реестра `issues.md`
* **Файлы:** Вся кодовая база решения
* **Задачи:**
  1. Полная пересборка решения в конфигурациях `Debug|x64`, `Release|x64`, `Release|x86`.
  2. Прогон сквозного сценария:
     - Запуск и проверка отображения только съемных накопителей.
     - Защита от ввода невалидных номеров и символов.
     - Запуск теста записи и чтения: контроль прогресса, скорости и отсутствия утечек памяти.
     - Проверка сохранности файлов истории (`UsbTestHistory.json`), импорт и экспорт.
     - Проверка отсутствия сетевых запросов при отказе от телеметрии.
  3. Финальный аудит соответствия всем пунктам [issues.md](file:///d:/Projects/usb-test/issues.md).
* **Критерии приемки:**
  - Решение собирается без ошибок и предупреждений компилятора (`0 Warning(s), 0 Error(s)`).
  - Все 14 дефектов из реестра устранены.
