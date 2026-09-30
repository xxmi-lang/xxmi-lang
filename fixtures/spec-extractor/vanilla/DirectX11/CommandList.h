// Synthetic vanilla-3DMigoto-shaped CommandList.h for spec-extractor tests.
static EnumName_t<const wchar_t *, VariableFlags> VariableFlagNames[] = {
	{L"global",  VariableFlags::GLOBAL},
	{L"persist", VariableFlags::PERSIST},
	{NULL,       VariableFlags::INVALID}
};

static EnumName_t<const wchar_t *, CustomResourceType> CustomResourceTypeNames[] = {
	{L"Buffer", CustomResourceType::BUFFER},
	{L"Texture2D", CustomResourceType::TEXTURE2D},
	{NULL, CustomResourceType::INVALID}
};


//static EnumName_t<const wchar_t*, Disabled> DisabledNames[] = {
//	{L"ignored", Disabled::X},
//};
