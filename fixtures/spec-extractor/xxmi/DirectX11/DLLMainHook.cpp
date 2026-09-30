// Synthetic XXMI-shaped DLLMainHook.cpp: early-startup reads through ini_parser_lite.
static bool verify_intended_target(const char *buf)
{
	const char *section = find_ini_section_lite(buf, "loader");
	if (!section)
		return true;
	if (find_ini_setting_lite(section, "loader", loader, MAX_PATH)) {
	}
	return find_ini_bool_lite(section, "check_version", false);
}
