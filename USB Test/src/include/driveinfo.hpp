#pragma once

#include "lang.hpp"
#include <vector>

extern std::vector<char> drives;

std::vector<char> getDrives();
ULARGE_INTEGER getDrivesInfo(std::vector<char>& drives, int selectedDrive, bool isTableNeeded);
void formatDisk(int selectedDrive);
DWORD getSectorSize(int selectedDrive, std::vector<char>& drives);
unsigned long long getTotalMegabytes(std::vector<char>& drives, int selectedDrive);