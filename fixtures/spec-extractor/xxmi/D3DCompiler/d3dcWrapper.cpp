// Synthetic D3DCompiler wrapper: reads d3dx.ini with GetPrivateProfile*.
static void LoadConfig(const wchar_t *dir)
{
	GetPrivateProfileString(L"Rendering", L"storage_directory", 0, SHADER_PATH, MAX_PATH, dir);
}
