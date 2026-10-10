#pragma once

#include "messages.hpp"

#include <string>

std::string urlEncode(const std::string& value);
void collectFeedback(int dialogAnswer);
int displayFeedbackDialog();
bool checkMsgAlreadyShown();
void editRegistry();
void sendFeedback(const std::string& feedback);