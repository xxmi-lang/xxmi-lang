// Synthetic XXMI-shaped CommandList.cpp for spec-extractor tests.
static bool ParseDrawCommand(const wchar_t *section, const wchar_t *key, wstring *val)
{
	if (!wcscmp(key, L"draw")) {
		if (!wcscmp(val->c_str(), L"from_caller")) {
			operation->type = DrawCommandType::FROM_CALLER;
		} else {
			ok = ParseDrawCommandArgs(section, val, operation, false, 2, ini_namespace, scope);
		}
	} else if (!wcscmp(key, L"drawindexed")) {
		if (!wcscmp(val->c_str(), L"auto")) {
			operation->type = DrawCommandType::AUTO_INDEX_COUNT;
		} else {
			ok = ParseDrawCommandArgs(section, val, operation, false, 3, ini_namespace, scope);
		}
	} else if (!wcscmp(key, L"dispatchindirect")) {
		ok = ParseDrawCommandArgs(section, val, operation, true, 1, ini_namespace, scope);
	}
}

bool ParseCommandListGeneralCommands(const wchar_t *section, const wchar_t *key, wstring *val)
{
	if (!wcscmp(key, L"run")) {
		if (!wcsncmp(val->c_str(), L"customshader", 12))
			return ParseRunShader(section, key, val);
		if (!wcsncmp(val->c_str(), L"commandlist", 11))
			return ParseRunExplicitCommandList(section, key, val);
	}
	if (!wcscmp(key, L"handling")) {
		if (!wcscmp(val->c_str(), L"skip"))
			return AddCommandToList(new SkipCommand(section));
	}
	if (!wcscmp(key, L"store")) {
		return ParseStoreCommand(section, key, val);
	}
	if (!wcsncmp(key, L"commandlist", 11))
		return ParseCopyCommandListCommand(section, key, val);
	return ParseDrawCommand(section, key, val);
}

bool ParseCommandListFlowControl(const wchar_t *section, const wstring *line)
{
	if (!wcsncmp(line->c_str(), L"if ", 3))
		return ParseIfCommand(section, line);
	if (!wcsncmp(line->c_str(), L"elif ", 5))
		return ParseElseIfCommand(section, line, 5);
	if (!wcscmp(line->c_str(), L"else"))
		return ParseElseCommand(section);
	if (!wcscmp(line->c_str(), L"endif"))
		return ParseEndIfCommand(section);
	return false;
}

bool ParseCommandListVariableAssignment(const wchar_t *section, const wchar_t *key, wstring *val)
{
	wstring line = key;
	bool declare_local = !line.compare(0, 5, L"local");
}

IniParserResult ResourceCopyTarget::ParseTargetMember(const wchar_t*& target, size_t& length)
{
	static constexpr MemberInfo members[] = {
		{ L"->size",   6, ResourceCopyTargetEvaluationMode::RESOURCE_SIZE },
		{ L"->region", 8, ResourceCopyTargetEvaluationMode::RESOURCE_REGION, {{
			MemberArg::Type::Unsigned,
			MemberArg::Type::Unsigned
		}} },
	};
	return SyntaxTarget::ParseTargetMember(members, target, length);
}

static const wchar_t *operator_tokens[] = {
	L"===", L"!==",
	L"<<", L">>", L"==", L"//", L"&&", L"||",
	L"(", L")", L"!", L"&", L"*", L"/", L"+", L"-",
};

DEFINE_OPERATOR(unary_not_operator,     "!",  (!rhs));
DEFINE_OPERATOR(unary_negate_operator,  "-",  (-rhs));
DEFINE_OPERATOR(sin_operator,           "sin", sin(rhs));
DEFINE_OPERATOR(multiplication_operator,"*",  (lhs * rhs));
DEFINE_OPERATOR(floor_division_operator,"//", (floor(lhs / rhs)));
DEFINE_OPERATOR(addition_operator,      "+",  (lhs + rhs));
DEFINE_OPERATOR(left_shift_operator,    "<<", ((int32_t)lhs << (int32_t)rhs));
DEFINE_OPERATOR(right_shift_operator,   ">>", ((int32_t)lhs >> (int32_t)rhs));
DEFINE_OPERATOR(bitwise_and_operator,   "&",  ((int32_t)lhs & (int32_t)rhs));
DEFINE_OPERATOR(and_operator,           "&&", (lhs && rhs));

static CommandListOperatorFactoryBase *unary_operators[] = {
	&unary_not_operator,
	&unary_negate_operator,
	&sin_operator,
};
static CommandListOperatorFactoryBase *multi_division_operators[] = {
	&multiplication_operator,
	&floor_division_operator,
};
static CommandListOperatorFactoryBase *add_subtract_operators[] = {
	&addition_operator,
};
static CommandListOperatorFactoryBase* shift_operators[] = {
	&left_shift_operator,
	&right_shift_operator,
};
static CommandListOperatorFactoryBase* bitwise_and_operators[] = {
	&bitwise_and_operator,
};
static CommandListOperatorFactoryBase *and_operators[] = {
	&and_operator,
};

bool CommandListExpression::parse(const wstring *expression)
{
	if (operator_mask & OP_UNARY)
		transform_operators_recursive(&tree, unary_operators, ARRAYSIZE(unary_operators), true, true);
	if (operator_mask & OP_MULTIPLICATION)
		transform_operators_recursive(&tree, multi_division_operators, ARRAYSIZE(multi_division_operators), false, false);
	if (operator_mask & OP_ADD_SUBTRACT)
		transform_operators_recursive(&tree, add_subtract_operators, ARRAYSIZE(add_subtract_operators), false, false);
	if (operator_mask & OP_SHIFT)
		transform_operators_recursive(&tree, shift_operators, ARRAYSIZE(shift_operators), false, false);
	if (operator_mask & OP_BITWISE_AND)
		transform_operators_recursive(&tree, bitwise_and_operators, ARRAYSIZE(bitwise_and_operators), false, false);
	if (operator_mask & OP_AND)
		transform_operators_recursive(&tree, and_operators, ARRAYSIZE(and_operators), false, false);
}
