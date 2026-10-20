#include "CommandList.h"

static bool ParseStoreCommand(const wchar_t *section, const wchar_t *key, wstring *val)
{
	CommandArgumentReader args(L"store", *val, section);
	return true;
}

bool ParseCommandListGeneralCommands(const wchar_t *section, const wchar_t *key, wstring *val)
{
	if (!wcscmp(key, L"store")) {
		return ParseStoreCommand(section, key, val);
	}
	return false;
}
