// Synthetic XXMI-shaped CommandList.h for spec-extractor tests.
static EnumName_t<const wchar_t *, VariableFlags> VariableFlagNames[] = {
	{L"global",  VariableFlags::GLOBAL},
	{L"persist", VariableFlags::PERSIST},
	{L"locked",  VariableFlags::LOCKED},
	{NULL,       VariableFlags::INVALID}
};

static EnumName_t<const wchar_t *, CustomResourceType> CustomResourceTypeNames[] = {
	{L"Buffer", CustomResourceType::BUFFER},
	{L"Texture2D", CustomResourceType::TEXTURE2D},
	{NULL, CustomResourceType::INVALID}
};

static EnumName_t<const wchar_t*, PoolIndexType> PoolIndexTypeNames[] = {
	{L"ring", PoolIndexType::RING},
	{L"static", PoolIndexType::STATIC},
	{NULL, PoolIndexType::INVALID}
};

//static EnumName_t<const wchar_t*, Disabled> DisabledNames[] = {
//	{L"ignored", Disabled::X},
//};
