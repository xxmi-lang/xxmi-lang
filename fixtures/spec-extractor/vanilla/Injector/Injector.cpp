// Synthetic Injector (3DMigoto Loader): reads [Loader] with the ini_parser_lite helpers.
int main()
{
	const char *ini_section = find_ini_section_lite(buf, "loader");
	if (!find_ini_setting_lite(ini_section, "module", module_path, MAX_PATH))
		return 1;
	if (find_ini_bool_lite(ini_section, "require_admin", false))
		elevate();
	find_ini_int_lite(ini_section, "delay", 0);
}
