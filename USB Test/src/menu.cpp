#include "include/menu.hpp"
#include <iostream>

using namespace std;
using json = nlohmann::json;

extern json jDrives;
extern bool isLangRu;

extern std::vector<char> drives;

extern HANDLE hConsole;
extern int menuCol, defCol, errCol;

/*
* @brief Displays main menu
*/
void mainMenu() {
	bool isRunning = true;
	while (isRunning) {
		system("title USB TEST - Main Menu");

		int option = 0;

		system("cls");

		SetConsoleTextAttribute(hConsole, menuCol);
		cout << "USB TEST v1.1.5 by n-romanovich \n\n";
		SetConsoleTextAttribute(hConsole, defCol);

		(isLangRu) ? cout << "1.\t" << "Начать тест \n" : cout << "1.\t" << "Start Test \n";
		(isLangRu) ? cout << "2.\t" << "История/Сравнение тестов \n" : cout << "2.\t" << "Test History/Comparison \n";
		(isLangRu) ? cout << "3.\t" << "Подробнее \n" : cout << "3.\t" << "About \n";
		(isLangRu) ? cout << "4.\t" << "Выход \n\n" : cout << "4.\t" << "Exit \n\n";

		(isLangRu) ? cout << "Введите номер пункта меню: " : cout << "Enter the menu option number: ";

		cin >> option;
		if (cin.fail()) {
			if (cin.eof()) {
				break;
			}
			msgInputError(false);
			continue;
		}

		switch (option) {
		default:
			msgInputError(false);
			break;

		case 1:		//Start test
			checkUpdates();
			menuTest();
			break;

		case 2:		//History
			checkUpdates();
			menuHistory();
			break;

		case 3:		//About
			checkUpdates();
			msgAbout();
			break;

		case 4:		//Exit
			isRunning = false;
			break;
		}
	}
}


/*
* @brief Displays test select menu
*/
void menuTest() {
	system("cls");

	drives = getDrives();

	int selectedDrive;
	int selectedTest;		//Selected test: 1 - Write only, 2 - Write, read, and compare	

	getDrivesInfo(drives, -1, true);		//Display list of drives, highlighting drive -1 as selected (no drive selected)

	if (drives.empty()) {
		system("pause");
		return;
	}

	selectedDrive = msgSelectDrive();		//Display drive selection message + Select logic

	system("cls");

	getDrivesInfo(drives, selectedDrive, true);		//Display drives list + highlighting selected drive

	selectedTest = msgSelectTestType(selectedDrive);		//Display list of tests + Select logic

	if (!msgConfirmTest(selectedDrive)) {		//Display test confirmation message
		return;
	}

	system("cls");
	driveTestWritePart(selectedDrive, drives, selectedTest);		//Perform write test. If "Write&Read" was selected, the read + compare part starts at the end of this function.

	system("cls");
}


/*
* @brief Displays history menu
*/
void menuHistory() {
	system("title USB TEST - Test History");

	while (true) {
		system("cls");
		int selectedOption = -1;

		if (isLangRu) {		//Russian
			SetConsoleTextAttribute(hConsole, menuCol);
			cout << "ИСТОРИЯ ТЕСТОВ";
			SetConsoleTextAttribute(hConsole, defCol);

			cout << "\n\n1. \tПросмотреть историю";
			cout << "\n2. \tСравнение тестов";
			cout << "\n3. \tИмпорт/экспорт";
			cout << "\n4. \tСтереть историю";
			cout << "\n5. \tНазад";
			cout << "\n\nВведите номер пункта меню: \n";
		}
		else {		//English
			SetConsoleTextAttribute(hConsole, menuCol);
			cout << "TEST HISTORY";
			SetConsoleTextAttribute(hConsole, defCol);

			cout << "\n\n1. \tView history";
			cout << "\n2. \tComparison";
			cout << "\n3. \tImport/export";
			cout << "\n4. \tErase data";
			cout << "\n5. \tBack";
			cout << "\n\nEnter the menu option number: \n";
		}
		cin >> selectedOption;

		if (cin.fail()) {
			if (cin.eof()) {
				return;
			}
			msgInputError(false);
			continue;
		}

		if (selectedOption > 5 || selectedOption < 1) {
			msgInputError(false);
			continue;
		}

		switch (selectedOption) {
		case 1:		//View history
			viewHistory();
			break;
		case 2:		//Compare
			menuComparison();
			break;
		case 3:		//Import/Export
			menuImportExport();
			break;
		case 4:		//Erase data
			if (eraseData()) {
				SetConsoleTextAttribute(hConsole, errCol);
				isLangRu ? cout << "\nИстория стерта!\n" : cout << "\nHistory erased!\n";
				SetConsoleTextAttribute(hConsole, defCol);
			}
			else {
				msgFileOpenError();
			}

			system("pause");
			break;
		case 5:		//Back
			return;
		}
	}
}


/*
* @brief Displays comparison menu
*/
void menuComparison() {
	system("title USB TEST - Comparison");

	int optionOne = 0, optionTwo = 0;		//Option: 1 - Test from history; 2 - Import test from file
	string optionOneValue, optionTwoValue;		//Test number in history or a file path

	//TEST ONE
	while (true) {
		system("cls");
		SetConsoleTextAttribute(hConsole, menuCol);
		(isLangRu) ? cout << "СРАВНЕНИЕ ТЕСТОВ - Выберите тест 1\n\n" : cout << "TEST COMPARISON - Select Test 1\n\n";
		SetConsoleTextAttribute(hConsole, defCol);

		(isLangRu) ? cout << "1. Тест из истории \n" : cout << "1. Test from History \n";
		(isLangRu) ? cout << "2. Импортировать из файла \n\n" : cout << "2. Import from File \n\n";

		(isLangRu) ? cout << "Введите номер пункта меню: " : cout << "Enter the menu option number: ";

		cin >> optionOne;
		if (cin.fail()) {
			if (cin.eof()) {
				return;
			}
			msgInputError(false);
			continue;
		}
		if (optionOne != 1 && optionOne != 2) {
			msgInputError(false);
			continue;
		}
		break;
	}

	readData();
	system("cls");

	if (optionOne == 1) {
		for (size_t i = 0; i < jDrives.size(); i++)
		{
			msgDisplayShortTestData(jDrives, static_cast<int>(i));
		}

		(isLangRu) ? cout << "\n\nВыберите тест: " : cout << "Select the Test: ";
		cin >> optionOneValue;
		if (cin.fail()) {
			cin.clear();
			cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		}
	}
	else {
		(isLangRu) ? cout << "Введите путь к файлу с тестом для импорта (например logs/Test1.json): " : cout << "Enter import file path (e.g. logs/test1.json): ";
		cin >> optionOneValue;
		if (cin.fail()) {
			cin.clear();
			cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		}
	}
	system("cls");

	//TEST TWO
	while (true) {
		system("cls");
		SetConsoleTextAttribute(hConsole, menuCol);
		(isLangRu) ? cout << "СРАВНЕНИЕ ТЕСТОВ - Выберите тест 2\n\n" : cout << "TEST COMPARISON - Select Test 2\n\n";
		SetConsoleTextAttribute(hConsole, defCol);

		(isLangRu) ? cout << "1. Тест из истории \n" : cout << "1. Test from History \n";
		(isLangRu) ? cout << "2. Импортировать из файла \n\n" : cout << "2. Import from File \n\n";

		(isLangRu) ? cout << "Введите номер пункта меню: " : cout << "Enter the menu option number: ";

		cin >> optionTwo;
		if (cin.fail()) {
			if (cin.eof()) {
				return;
			}
			msgInputError(false);
			continue;
		}
		if (optionTwo != 1 && optionTwo != 2) {
			msgInputError(false);
			continue;
		}
		break;
	}
	system("cls");

	if (optionTwo == 1) {
		for (size_t i = 0; i < jDrives.size(); i++)
		{
			msgDisplayShortTestData(jDrives, static_cast<int>(i));
		}

		(isLangRu) ? cout << "\n\nВыберите тест: " : cout << "Select the Test: ";
		cin >> optionTwoValue;
		if (cin.fail()) {
			cin.clear();
			cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		}
	}
	else {
		(isLangRu) ? cout << "Введите путь к файлу с тестом для импорта (например logs/test1.json): " : cout << "Enter import file path (e.g. logs/test1.json): ";
		cin >> optionTwoValue;
		if (cin.fail()) {
			cin.clear();
			cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		}
	}
	system("cls");

	compareTests(optionOne, optionTwo, optionOneValue, optionTwoValue);
}