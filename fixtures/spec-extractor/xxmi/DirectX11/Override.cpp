// Synthetic XXMI-shaped Override.cpp for spec-extractor tests.
void Override::ParseIniSection(LPCWSTR section)
{
	for (entry = section_vec->begin(); entry < section_vec->end(); entry++) {
		if (ParseIniParamName(entry->first.c_str(), &param_idx, &param_component)) {
			val = GetIniFloat(section, entry->first.c_str(), FLT_MAX, NULL);
		} else if (entry->first.c_str()[0] == L'$') {
			val = GetIniFloat(section, entry->first.c_str(), FLT_MAX, NULL);
		}
	}
	transition = GetIniInt(section, L"transition", 0, NULL);
	if (GetIniStringAndLog(section, L"condition", 0, buf, MAX_PATH)) {
	}
}

void KeyOverrideCycle::ParseIniSection(LPCWSTR section)
{
	wrap = GetIniBool(section, L"wrap", true, NULL);
}
