#include "include/json_utils.hpp"
#include <iostream>
#include <fstream>
#include <limits>
#include <shlobj.h>

using namespace std;
using json = nlohmann::json;	//nlohmann's JSON lib

/*
* @brief Validates that the JSON object represents a valid test record with all required fields
*/
bool isValidTestRecord(const json& item) {
	if (!item.is_object()) {
		return false;
	}
	const char* requiredKeys[] = {
		"drive_letter", "total_mb", "slow_count", "very_slow_count",
		"crit_count", "bad_count", "crit_count_read", "bad_count_read",
		"write_speed", "read_speed"
	};
	for (const char* key : requiredKeys) {
		if (!item.contains(key)) {
			return false;
		}
	}
	return true;
}

json jDrives = json::array();	//Creating JSON array

const string& getDataPath() {
	static const string dataPath = []() {
		string docs = getDocumentsPath();
		if (docs.empty()) {
			return string("UsbTestHistory.json");
		}
		return docs + "\\" + "UsbTestHistory.json";
	}();
	return dataPath;
}

extern bool isLangRu;

extern HANDLE hConsole;
extern int menuCol, defCol, errCol;

/*
* @brief Reads data from file and pushes it to jDrives
* 
* @return Returns true/false
*/
bool readData() {

	ifstream file(getDataPath());	//Open file
	if (!file.is_open()) {
		jDrives = json::array();
		return false;
	}

	if (file.peek() == EOF) {
		file.close();
		jDrives = json::array();
		return false;
	}

	try {
		file >> jDrives;
		file.close();
		if (!jDrives.is_array()) {
			jDrives = json::array();
		}
		return true;
	}
	catch (const json::parse_error&) {
		file.close();
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cerr << "Ошибка чтения файла истории: поврежденный JSON." << endl
		           : cerr << "Error reading history file: corrupted JSON." << endl;
		SetConsoleTextAttribute(hConsole, defCol);
		jDrives = json::array();
		return false;
	}
	catch (const exception& e) {
		file.close();
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cerr << "Ошибка чтения файла истории: " << e.what() << endl
		           : cerr << "Error reading history file: " << e.what() << endl;
		SetConsoleTextAttribute(hConsole, defCol);
		jDrives = json::array();
		return false;
	}
}


/*
* @brief Pushes new data to jDrives array
*
* @param
* string driveLetter: tested drive letter
* 
* unsigned long long totalMegabytes: tested drive capacity in megabytes
* 
* int ... : slow, very slow, critically slow and bad sectors count
* 
* double writeSpeed: drive write speed
* 
* double readSpeed: drive read speed
*
* @return Returns true/false
*/
bool pushBackData(string driveLetter, unsigned long long totalMegabytes, int slowCount, int verySlowCount, int critCount, int critCountRead, int badCount, int badCountRead, double writeSpeed, double readSpeed) {

	readData();

	//Pushing new data to jDrives
	jDrives.push_back({
		{"drive_letter", driveLetter},
		{"total_mb", totalMegabytes},
		{"slow_count", slowCount},
		{"very_slow_count", verySlowCount},
		{"crit_count", critCount},
		{"bad_count", badCount},
		{"crit_count_read", critCountRead},
		{"bad_count_read", badCountRead},
		{"write_speed", writeSpeed},
		{"read_speed", readSpeed}
	});

	pushDrivesArray();	//Pushing jDrives into .json file

	return true;
}


/*
* @brief Push jDrives data into .json file
*
* @return Returns true/false
*/
bool pushDrivesArray() {

	ofstream file(getDataPath());
	if (!file.is_open()) {
		return false;
	}

	file << jDrives;

	file.close();
	return true;
}


/*
* @brief Displays specific test data
*
* @param
* selected test number (1 - write, 2 - write and read)
*/
void displayTestData(int selectedTest) {

	readData();

	msgDisplayTestData(jDrives, selectedTest, -1);	//Display data
}


/*
* @brief Exports test data from file
*
* @return Returns true/false
*/
bool exportData() {
	readData();

	if (jDrives.empty()) {
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cout << "История тестов пуста. Экспорт невозможен.\n" : cout << "Test history is empty. Export cannot be performed.\n";
		SetConsoleTextAttribute(hConsole, defCol);
		system("pause");
		return false;
	}

	int testNumber;
	string exportFilePath;
	
	system("cls");
	//Displaying all tests (short form)
	for (size_t i = 0; i < jDrives.size(); i++)
	{
		msgDisplayShortTestData(jDrives, static_cast<int>(i));
	}
	cout << "\n\n";

	(isLangRu) ? cout << "Введите номер теста, который хотите экспортировать: " : cout << "Enter the test number to export: ";
	cin >> testNumber; 

	if (cin.fail() || testNumber < 1 || testNumber > static_cast<int>(jDrives.size())) {
		cin.clear();
		cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		msgInputError(false);
		return false;
	}
	testNumber--;

	cout << endl;

	(isLangRu) ? cout << "Введите путь к файлу, в который будет экспортирован тест (например logs/test1.json): " : cout << "Enter the export file path (e.g. logs/test1.json): ";
	cin >> exportFilePath;
	if (cin.fail()) {
		cin.clear();
		cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		msgInputError(false);
		return false;
	}
	cout << endl;

	//Opening export file
	ofstream exportFile(exportFilePath);
	if (exportFile.is_open()) {
		exportFile << json::array({ jDrives[testNumber] });		//Pushing specific test to the file
		exportFile.close();

		SetConsoleTextAttribute(hConsole, 10);
		(isLangRu) ? cout << "Тест успешно экспортирован!\n\n" : cout << "Test exported successfully!\n\n";
		SetConsoleTextAttribute(hConsole, defCol);
		system("pause");
		return true;
	}
	else { 
		msgFileOpenError();
		return false; 
	}
}


/*
* @brief Imports test data to file
*
* @return Returns true/false
*/
bool importData() {
	string importFilePath;
	int addToHistory = -1;
	json jTestImport = json::array();
	
	system("cls");
	(isLangRu) ? cout << "Введите путь к файлу с тестом для импорта (например logs/test1.json): " : cout << "Enter import file path (e.g. logs/test1.json): ";
	cin >> importFilePath;
	if (cin.fail()) {
		cin.clear();
		cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		msgInputError(false);
		return false;
	}
	cout << endl;

	ifstream file(importFilePath);
	if (!file.is_open()) {
		msgFileOpenError();
		return false;
	}

	try {
		file >> jTestImport;
	}
	catch (const exception&) {
		file.close();
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cout << "Ошибка! Некорректный синтаксис JSON в файле импорта." << endl
		           : cout << "Error! Invalid JSON syntax in import file." << endl;
		SetConsoleTextAttribute(hConsole, defCol);
		system("pause");
		return false;
	}
	file.close();

	if (!jTestImport.is_array() || jTestImport.empty() || !isValidTestRecord(jTestImport[0])) {
		SetConsoleTextAttribute(hConsole, errCol);
		(isLangRu) ? cout << "Ошибка! Файл импорта пуст или содержит некорректную структуру записи теста." << endl
		           : cout << "Error! Import file is empty or contains an invalid test record structure." << endl;
		SetConsoleTextAttribute(hConsole, defCol);
		system("pause");
		return false;
	}

	system("cls");
	msgDisplayTestData(jTestImport, 0, -1);	//Displaying data from file

	(isLangRu) ? cout << "\n\nДобавить тест в историю?\n" : cout << "\n\nAdd test to history?\n";
	(isLangRu) ? cout << "1. Да\t2. Нет\n" : cout << "1.Yes\t2. No\n";

	cin >> addToHistory;
	if (cin.fail() || (addToHistory != 1 && addToHistory != 2)) {
		cin.clear();
		cin.ignore((numeric_limits<streamsize>::max)(), '\n');
		msgInputError(false);
		return false;
	}

	if (addToHistory == 1) {
		readData();

		//Pushing data to jDrives array
		jDrives.push_back({
			{"drive_letter", jTestImport[0]["drive_letter"]},
			{"total_mb", jTestImport[0]["total_mb"]},
			{"slow_count", jTestImport[0]["slow_count"]},
			{"very_slow_count", jTestImport[0]["very_slow_count"]},
			{"crit_count", jTestImport[0]["crit_count"]},
			{"bad_count", jTestImport[0]["bad_count"]},
			{"crit_count_read", jTestImport[0]["crit_count_read"]},
			{"bad_count_read", jTestImport[0]["bad_count_read"]},
			{"write_speed", jTestImport[0]["write_speed"]},
			{"read_speed", jTestImport[0]["read_speed"]}
		});

		pushDrivesArray();	//Pushing jDrives array into file

		SetConsoleTextAttribute(hConsole, 10);
		(isLangRu) ? cout << "Тест успешно добавлен в историю!\n\n" : cout << "Test added to history successfully!\n\n";
		SetConsoleTextAttribute(hConsole, defCol);
	}

	system("pause");
	return true;
}


/*
@brief Compares data test data

@param 
json jComparisonOne: JSON array with test data
json jComparisonTwo: JSON array with test data

@return Returns True/False
*/
bool compareData(json jComparisonOne, json jComparisonTwo) {

	SetConsoleTextAttribute(hConsole, menuCol);
	isLangRu ? cout << "СРАВНЕНИЕ ТЕСТОВ \n\n" : cout << "TEST COMPARISON \n\n";
	SetConsoleTextAttribute(hConsole, defCol);

	msgDisplayTestData(jComparisonOne, 0, 1);

	cout << "\n\n";

	msgDisplayTestComparison(jComparisonOne, jComparisonTwo);
	system("pause");

	return true;
}


/*
* @brief Erases history
*
* @return Returns true/false
*/
bool eraseData() {
	
	ofstream fileOut(getDataPath(), ios::trunc);
	fileOut.close();

	ifstream fileIn(getDataPath());
	if (fileIn.is_open() && fileIn.peek() == EOF) {
		fileIn.close();
	}
	else {
		fileIn.close();
		return false;
	}

	jDrives = json::array();	//Clean the jDrives array
	return true;
}


/*
* @brief Displays specific test data
*
* @return Returns true/false
*/
string getDocumentsPath() {
	
	char path[MAX_PATH];	//260 symbol path

	HRESULT hr = SHGetFolderPathA(NULL, CSIDL_PERSONAL, NULL, SHGFP_TYPE_CURRENT, path);	//Getting Documents folder path

	if (SUCCEEDED(hr)) {
		return string(path);	//Returning string with the Documents folder path
	}
	else
	return "";
}