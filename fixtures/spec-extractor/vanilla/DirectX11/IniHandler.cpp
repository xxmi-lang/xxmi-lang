// Synthetic vanilla-3DMigoto-shaped IniHandler.cpp for spec-extractor tests.
struct Section {
	wchar_t *section;
	bool prefix;
};
static Section CommandListSections[] = {
	{L"TextureOverride", true},
	{L"CommandList", true},
	{L"Constants", false},
};
static Section RegularSections[] = {
	{L"Resource", true},
	{L"Key", true},
	{L"Include", true},
	{L"Logging", false},
	{L"Loader", false},
};
static Section AllowLinesWithoutEquals[] = {
	{L"Profile", false},
};

static bool whitelisted_duplicate_key(const wchar_t *section, const wchar_t *key)
{
	if (!_wcsnicmp(section, L"key", 3)) {
		if (!_wcsicmp(key, L"key") || !_wcsicmp(key, L"back"))
			return true;
	}
	if (!_wcsicmp(section, L"include"))
		return true;
	return false;
}

static bool DoesSectionAllowLinesWithoutEquals(const wchar_t *section)
{
	return SectionInList(section, AllowLinesWithoutEquals, ARRAYSIZE(AllowLinesWithoutEquals))
		|| IsCommandListSection(section);
}

static void ParseIniSectionLine(wstring *wline, wstring *section, int *warn_duplicates)
{
	if (IsCommandListSection(section->c_str())) {
		if (*warn_duplicates == 1)
			*warn_duplicates = 0;
	}
}

static void ParseIncludedIniFiles()
{
	/* A comment with L"not_a_key" must be ignored. */
	switch (key->size()) {
	case 7:
		if (!wcscmp(key->c_str(), L"include"))
			continue;
		break;
	case 17:
		if (!wcscmp(key->c_str(), L"include_recursive"))
			continue;
		break;
	}
	GetIniString(L"Include", L"user_config", L"d3dx_user.ini", &tmp);
}

static void ParseResourceSections()
{
	custom_resource->max_copies_per_frame = GetIniInt(section_name, L"max_copies_per_frame", 0, NULL);
	if (GetIniStringAndLog(section_name, L"filename", 0, setting, MAX_PATH)) {
		// L"commented_out" is not a key
	}
	custom_resource->override_type = GetIniEnumClass(section_name, L"type", CustomResourceType::INVALID, NULL, CustomResourceTypeNames);
	if (GetIniString(section_name, L"format", 0, setting, MAX_PATH)) {
		custom_resource->override_format = ParseFormatString(setting, true);
	}
	custom_resource->override_width = GetIniInt(section_name, L"width", -1, NULL);
	custom_resource->override_mode = GetIniEnum(section_name, L"mode", 0, NULL, L"mono", StereoModeNames, 3, 0);
}

static void ParseResourceInitialData(CustomResource *custom_resource, const wchar_t *section)
{
	if (!GetIniStringAndLog(section, L"data", 0, setting, MAX_PATH))
		return;
}


#define TEXTURE_OVERRIDE_FUZZY_MATCHES \
	L"match_type", \
	L"match_format"

wchar_t *TextureOverrideIniKeys[] = {
	L"hash",
	L"match_priority",
	TEXTURE_OVERRIDE_FUZZY_MATCHES,
	NULL
};

wchar_t *TextureOverrideFuzzyMatchesIniKeys[] = {
	TEXTURE_OVERRIDE_FUZZY_MATCHES,
	NULL
};

static void ParseTextureOverrideSections()
{
	hash = (uint32_t)GetIniHash(id, L"Hash", 0, &found);
}

static void parse_texture_override_common(const wchar_t *id, TextureOverride *override, bool register_command_lists)
{
	override->priority = GetIniInt(id, L"match_priority", 0, &found);
	// Not whitelisted, so parsed as a command, not a key:
	GetIniInt(id, L"not_whitelisted", 0, NULL);
}

static void parse_texture_override_fuzzy_match(const wchar_t *section)
{
	fuzzy->Type = GetIniEnumClass(section, L"match_type", D3D11_RESOURCE_DIMENSION_UNKNOWN, NULL, ResourceDimensionNames);
	if (GetIniStringAndLog(section, L"match_format", 0, setting, MAX_PATH)) {
		fuzzy->Format.val = ParseFormatString(setting, true);
	}
}

static void RegisterPresetKeyBindings()
{
	keys = GetIniStringMultipleKeys(id, L"Key");
	back = GetIniStringMultipleKeys(id, L"Back");
	type = GetIniEnumClass(id, L"type", KeyOverrideType::ACTIVATE, NULL, KeyOverrideTypeNames);
}

static void ParseCommandList(const wchar_t *id, CommandList *pre_command_list, CommandList *post_command_list)
{
	if (post_command_list) {
		if (!key->compare(0, 5, L"post ")) {
			key_ptr += 5;
		} else if (!key->compare(0, 4, L"pre ")) {
			key_ptr += 4;
		}
	}
}

void LoadConfigFile()
{
	gLogDebug = GetIniBool(L"Logging", L"debug", false, NULL);
}

static void InsertBuiltInIniSections()
{
	static const wchar_t text[] =
		L"[BuiltInCommandListUnbindAllRenderTargets]\n"
		L"o0 = null\n"
	;
	ParseIniExcerpt(text);
}
