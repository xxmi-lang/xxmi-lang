#include "tree_sitter/parser.h"
#include "tree_sitter/alloc.h"
#include "tree_sitter/array.h"

#include <wctype.h>
#include <string.h>

#define strlwr _tolower_str
#define scan_section_header_prefix scan_maybe_section_header

typedef enum {
    EXTERNAL_LINE,
    SECTION_HEADER_START,
    SECTION_HEADER_GUARD,
    POOLED_VARIABLE_START,
    POOLED_VARIABLE_GUARD,
    KEY_HEADER_PREFIX,
    REGEX_HEADER_PREFIX,
    PRESET_HEADER_PREFIX,
    INCLUDE_HEADER_PREFIX,
    COMMANDLIST_HEADER_PREFIX,
    COMMANDLIST_CALLABLE_PREFIX,
    CUSTOMSHADER_CALLABLE_PREFIX,
    CUSTOMRESOURCE_HEADER_PREFIX,
    RESOURCEPOOL_HEADER_PREFIX,
    NAMESPACE_RESOLUTION_START,
    NAMESPACE_RESOLUTION_CONTENT,
    NAMESPACE_RESOLUTION_END,
    SUFFIXED_KEY_HEADER,
    SUFFIXED_PRESET_HEADER,
    // (?:Resource|Pool)[^\]]+
    SUFFIXED_RESOURCE_HEADER,
    // Include[^\]]*
    SUFFIXED_INCLUDE_HEADER,
    // (?:ShaderOverride|TextureOverride|(?:BuiltIn)?(?:CommandList|CustomShader))[^\]]+
    SUFFIXED_COMMANDLIST_HEADER,
    REGEX_COMMANDLIST_HEADER,
    REGEX_DECLARATIONS_HEADER,
    REGEX_PATTERN_HEADER,
    REGEX_REPLACE_HEADER,
    NEWLINE,
    ANTIWHITESPACE,
    DOC_COMMENT_CONTENT,
    CUSTOM_RESOURCE_IDENTIFIER,
    ERROR_SENTINEL,
} TokenType;

const char *SECTION_PREFIX_NAMES[] = {
    "shaderoverride",
    "shaderregex",
    "customshader", // + builtincustomshader
    "commandlist",  // + builtincommandlist
    "textureoverride",
    "resource",
    "include",
    "preset",
    "pool",
    "key",
    0
};

// Intentionally leaves out a few names with unique first letters
const char *SECTION_NAMES[] = {
    "shaderoverride",
    "shaderregex",
    "customshader",
    "commandlist",
    "builtincustomshader",
    "builtincommandlist",
    "present",
    "clearrendertargetview",
    "cleardepthstencilview",
    "clearunorderedaccessviewuint",
    "clearunorderedaccessviewfloat",
    "constants",
    "logging",
    "system",
    "stereo",
    "rendering",
    "profile",
    "convergencemap",
    "resource",
    "pool",
    "preset",
    "loader",
    "input",
    "include",
    0
};

typedef enum {
    NOTSEARCHING,
    GENSEARCH = 4,
    INSSEARCH,
    PATSEARCH,
    REPSEARCH,
} SearchState;

typedef Array(char) String;

typedef struct {
    String word;
} Scanner;

static inline void reset(Scanner *scanner) {
    array_delete(&scanner->word);
    array_init(&scanner->word);
}

void *tree_sitter_migoto_external_scanner_create() {
    Scanner *scanner = ts_calloc(1, sizeof(Scanner));
    array_reserve(&scanner->word, 256);
    return scanner;
}

void tree_sitter_migoto_external_scanner_destroy(void *payload) {
    Scanner *scanner = (Scanner*) payload;
    array_delete(&scanner->word);
    ts_free(scanner);
}

unsigned tree_sitter_migoto_external_scanner_serialize(void *payload, char *buffer) {
    unsigned size = 0;
    Scanner *scanner = (Scanner*) payload;

    if (scanner->word.size * sizeof(char) + 1 >= TREE_SITTER_SERIALIZATION_BUFFER_SIZE) {
        return 0;
    }

    buffer[size++] = (char) scanner->word.size;
    memcpy(&buffer[size], scanner->word.contents, scanner->word.size);
    size += scanner->word.size;
    return size;
}

void tree_sitter_migoto_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    unsigned size = 0;
    Scanner *scanner = (Scanner*) payload;

    reset(scanner);

    if (length == 0) return;

    scanner->word = (String)array_new();
    uint8_t word_length = buffer[size++];
    array_reserve(&scanner->word, word_length);
    memcpy(scanner->word.contents, &buffer[size], word_length);
    scanner->word.size = word_length;
    size += word_length;

    assert(size == length);
}

static inline void consume(TSLexer *lexer) { lexer->advance(lexer, false); }

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static inline bool is_eof(TSLexer *lexer) { return lexer->eof(lexer); }

static inline void mark_end(TSLexer *lexer) { lexer->mark_end(lexer); }

/// Lowercase a string in place
static inline char *_tolower_str(char *_Str) {
    size_t i = 0;

    while (_Str[i] != '\0') {
        _Str[i] = (char) towlower(_Str[i]);
        i++;
    }

    return _Str;
}

/// Determines if search_term exactly matches one of the section names in search_arr,
/// using multiple sizes to test multiple options
/// @returns An integer representing the 1-indexed position in the sizes array of a match, and 0 otherwise.
static inline int is_section_name_l(char *search_term, uint8_t *sizes, const char **search_arr) {
    strlwr(search_term); // mutates search_term
    for(int i = 0; search_arr[i]; i++) {
        for (int j = 0; sizes[j]; j++) {
            if(!strncmp(search_term, search_arr[i], sizes[j])) {
                return j+1;
            }
        }
    }
    return 0;
}

static inline bool scan_end_of_line(TSLexer *lexer) {
    // fprintf(stderr, "[Lykare]: lookahead for newline\n");
    bool found_end_of_line = false;
    mark_end(lexer);

    for (;;) {
        // fprintf(stderr, "[Lykare]: starting loop iteration\n");
        if (lexer->lookahead == '\n') {
            // fprintf(stderr, "[Lykare]: found newline\n");
            found_end_of_line = true;
            skip(lexer);
            break;
        }
        else if (iswspace(lexer->lookahead) && lexer->lookahead != '\n') {
            // fprintf(stderr, "[Lykare]: found non-newline whitespace\n");
            skip(lexer);
        }
        else if (is_eof(lexer)) {
            // fprintf(stderr, "[Lykare]: found EOF\n");
            found_end_of_line = true;
            break;
        }
        else {
            // fprintf(stderr, "[Lykare]: found text, mark end\n");
            mark_end(lexer);
            break;
        }
    }

    if (found_end_of_line) {
        // fprintf(stderr, "[Lykare]: found an end to the line\n");
        lexer->result_symbol = NEWLINE;
        return true;
    }

    return false;
}

static inline bool scan_namespace_res_start_end(TSLexer *lexer, unsigned position) {
    if (lexer->lookahead != '\\') {
        return false;
    }

    consume(lexer);

    bool res = true;

    // bake antiwhitespace detection into the rule for the end of namespace resolution
    if (position == NAMESPACE_RESOLUTION_END) {
        res = !iswspace(lexer->lookahead);
    }

    lexer->result_symbol = position;
    return res;
}

static inline bool scan_namespace_res_content(TSLexer *lexer) {
    bool index_flag = false;

    for (;;) {
        if (is_eof(lexer)) {
            return false;
        }

        switch (lexer->lookahead) {
        case ']':
            if (index_flag) {
                index_flag = false;
            }
            consume(lexer);
            break;
        case '[':
            if (!index_flag) {
                index_flag = true;
            }
            consume(lexer);
            break;
        case '$':
            consume(lexer);
            if (lexer->lookahead != '\\' || index_flag) {
                continue;
            }
            else {
                return false;
            }
            break;
        case '\\':
            if (!index_flag) {
                mark_end(lexer);
            }
            consume(lexer);
            break;
        case '=':
        case '&':
        case '|':
        case '+':
        case '-':
        case '/':
        case '*':
        case '<':
        case '>':
        case '%':
        case '!':
        case '\n':
            lexer->result_symbol = NAMESPACE_RESOLUTION_CONTENT;
            consume(lexer);
            return true;
        default:
            consume(lexer);
            break;
        }
    }
}

static inline bool scan_maybe_section_header(Scanner *scanner, TSLexer *lexer, const bool *valid_symbols, bool is_prefix_search) {
    // Prefix Search indicates that we are looking to return a specific token for the prefix
    // instead of returning a generic "this is a valid header name" for a single token
    if (!is_prefix_search) {
        consume(lexer); // consume the '[' that starts
    }

    // There should only be one starting newline if the prefix starts a line
    if (is_prefix_search && lexer->lookahead == '\n') {
        skip(lexer);
    }

    // consume all the leading whitespace
    while (iswspace(lexer->lookahead) && !is_eof(lexer)) {
        // if it's a prefix search we should skip the leading whitespace and not include it in the final token
        if (is_prefix_search && (
            valid_symbols[COMMANDLIST_CALLABLE_PREFIX] ||
            valid_symbols[CUSTOMSHADER_CALLABLE_PREFIX] ||
            valid_symbols[CUSTOMRESOURCE_HEADER_PREFIX] ||
            valid_symbols[RESOURCEPOOL_HEADER_PREFIX]
        ))
        {
            skip(lexer);
        }
        else
        {
            consume(lexer);
        }
    }

    // if after consuming whitespace we're at a terminal or eof, return false
    if (lexer->lookahead == ']' || lexer->lookahead == '\n' || is_eof(lexer)) {
        return false;
    }

    // if the next character is not alphabetical, return false
    // if it is alphabetical, but not the first character of a section name, return false
    // otherwise we can start searching
    // would love to just use strchr here, but wasm with wasi-sdk doesn't have support for it
    if (!iswalpha(lexer->lookahead) ||
        memchr("stcbplrhki", towlower(lexer->lookahead), 11) == NULL)
    {
        return false;
    }

    // must be sorted in descending order
    uint8_t search_lengths[7] = {0, 0, 0, 0, 0, 0, 0};
    char *target_term = 0;

    // choose the search length or set target term based on first character
    // s [6,11,14], c [11,12,14,21,28,29], p [4,6,7], b [18,19], l [6,7], r [8,9], i [7,5]: all have multiple options for header
callable:
    switch (towlower(lexer->lookahead)) {
    case 's':
        if (is_prefix_search) {
            // If we see 's' and it's a prefix search, it could be "ShaderRegex" or "ShaderOverride"
            if (valid_symbols[REGEX_HEADER_PREFIX] ||
                valid_symbols[COMMANDLIST_HEADER_PREFIX])
            {
                // we need to perform lookahead to the difference in possibilities
                // and be sure to collect what we consume in the scanner word
                for (int i = 0; i < 6; i++) {
                    array_push(&scanner->word, lexer->lookahead);
                    consume(lexer);
                }
                array_push(&scanner->word, '\0'); // NUL term char array before comparison

                if (!strcmp(strlwr(scanner->word.contents), "shader")) {
                    if (towlower(lexer->lookahead) == 'r') {
                        lexer->result_symbol = REGEX_HEADER_PREFIX;
                        target_term = "shaderregex";
                        search_lengths[0] = 11; // ShaderRegex
                    }
                    else if (towlower(lexer->lookahead) == 'o') {
                        lexer->result_symbol = COMMANDLIST_HEADER_PREFIX;
                        target_term = "shaderoverride";
                        search_lengths[0] = 14; // ShaderOverride
                    }
                    else {
                        return false;
                    }
                    array_pop(&scanner->word); // remove the null terminator before proceeding
                }
                else {
                    return false;
                }
            }
            else {
                return false;
            }
        }
        else {
            // it's not a prefix search, just go through the possibilities as normal
            search_lengths[0] = 14; // ShaderOverride
            search_lengths[1] = 11; // ShaderRegex
            search_lengths[2] = 6;  // System
        }
        break;
    case 't':
        // only TextureOverride starts with 't' of the prefixed headers
        if (is_prefix_search) lexer->result_symbol = COMMANDLIST_HEADER_PREFIX;
        target_term = "textureoverride";
        search_lengths[0] = 15;
        break;
    case 'c':
        if (is_prefix_search) {
            // If we see 'c' and it's a prefix search, it could be "CommandList" or "CustomShader"
            if (valid_symbols[CUSTOMSHADER_CALLABLE_PREFIX] ||
                valid_symbols[COMMANDLIST_CALLABLE_PREFIX])
            {
                // If we are looking for the callables, we need to perform an extra lookahead
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer); // consume the 'c'
                if (towlower(lexer->lookahead) == 'u') {
                    lexer->result_symbol = CUSTOMSHADER_CALLABLE_PREFIX;
                    target_term = "customshader";
                    search_lengths[0] = 12; // CustomShader
                }
                else if (towlower(lexer->lookahead) == 'o') {
                    lexer->result_symbol = COMMANDLIST_CALLABLE_PREFIX;
                    target_term = "commandlist";
                    search_lengths[0] = 11; // CommandList
                }
                else {
                    return false;
                }
            }
            else if (valid_symbols[COMMANDLIST_HEADER_PREFIX]) {
                lexer->result_symbol = COMMANDLIST_HEADER_PREFIX;
                search_lengths[0] = 12; // CustomShader
                search_lengths[1] = 11; // CommandList
            }
            else {
                return false;
            }
        }
        else {
            search_lengths[0] = 29; // ClearUnorderedAccessViewFloat
            search_lengths[1] = 28; // ClearUnorderedAccessViewUint
            search_lengths[2] = 21; // ClearRenderTargetView / ClearDepthStencilView
            search_lengths[3] = 14; // ConvergenceMap
            search_lengths[4] = 12; // CustomShader
            search_lengths[5] = 11; // CommandList
        }
        break;
    case 'b':
        // This is just the 'c' case but we need to confirm and consume the "BuiltIn" part first
        // Only really necessary for a prefix search, prefixed-header search can just proceed as normal
        if (is_prefix_search) {
            for (int i = 0; i < 7; i++) {
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer);
            }
            array_push(&scanner->word, '\0'); // NUL term char array before comparison

            if (!strcmp(strlwr(scanner->word.contents), "builtin")) {
                reset(scanner);
                goto callable;
            }
            else {
                return false;
            }
        }
        else {
            search_lengths[0] = 19; // BuiltInCustomShader
            search_lengths[1] = 18; // BuiltInCommandList
        }
        break;
    case 'p':
        if (is_prefix_search) {
            // If we see 'p' and it's a prefix search, it could be "Pool" or "Preset"
            if (valid_symbols[RESOURCEPOOL_HEADER_PREFIX] ||
                valid_symbols[PRESET_HEADER_PREFIX])
            {
                // Perform an extra lookahead
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer); // consume the 'p'
                if (towlower(lexer->lookahead) == 'r') {
                    lexer->result_symbol = PRESET_HEADER_PREFIX;
                    target_term = "preset";
                    search_lengths[0] = 6;
                }
                else if (towlower(lexer->lookahead) == 'o') {
                    lexer->result_symbol = RESOURCEPOOL_HEADER_PREFIX;
                    target_term = "pool";
                    search_lengths[0] = 4;
                }
                else {
                    return false;
                }
            }
            else {
                return false;
            }
        }
        else {
            search_lengths[0] = 7; // Present, Profile
            search_lengths[1] = 6; // Preset
            search_lengths[2] = 4; // Pool
        }
        break;
    case 'l':
        // There are no prefixes that start with 'l'
        if (is_prefix_search) return false;
        search_lengths[0] = 7; // Logging
        search_lengths[1] = 6; // Loader
        break;
    case 'd':
        // There are no prefixes that start with 'd'
        if (is_prefix_search) return false;
        target_term = "device";
        search_lengths[0] = 6;
        break;
    case 'r':
        if (is_prefix_search) {
            lexer->result_symbol = CUSTOMRESOURCE_HEADER_PREFIX;
            target_term = "resource";
            search_lengths[0] = 8;
        }
        else {
            search_lengths[0] = 9; // Rendering
            search_lengths[1] = 8; // Resource
        }
        break;
    case 'h':
        // There are no prefixes that start with 'h'
        if (is_prefix_search) return false;
        target_term = "hunting";
        search_lengths[0] = 7;
        break;
    case 'k':
        if (is_prefix_search) lexer->result_symbol = KEY_HEADER_PREFIX;
        target_term = "key";
        search_lengths[0] = 3;
        break;
    case 'i':
        if (is_prefix_search) {
            lexer->result_symbol = INCLUDE_HEADER_PREFIX;
            target_term = "include";
            search_lengths[0] = 7;
        }
        else {
            search_lengths[0] = 7; // Include
            search_lengths[1] = 5; // Input
        }
        break;
    default:
        return false;
    }

    char lookahead = lexer->lookahead;

    do {
        // if we see a terminal before collecting the longest number of characters
        if (lookahead == ']' || lookahead == '\n' || is_eof(lexer)) {
            break;
        }

        array_push(&scanner->word, lookahead);
        consume(lexer);
        // Should only happen with the COMMANDLIST_HEADER_PREFIX 'c' situation
        if (!target_term && scanner->word.size == search_lengths[1] && is_prefix_search) {
            mark_end(lexer);
        }
        lookahead = lexer->lookahead;
    } while (scanner->word.size < search_lengths[0]);

    if (is_prefix_search && target_term) mark_end(lexer);

    array_push(&scanner->word, '\0'); // NUL term char array before comparison

    if (target_term && !strcmp(strlwr(scanner->word.contents), target_term)) {
        reset(scanner);
        return true;
    }
    else if (!target_term) {
        int match_idx = 0;
        match_idx =
            is_section_name_l(
                scanner->word.contents,
                search_lengths,
                is_prefix_search ? SECTION_PREFIX_NAMES : SECTION_NAMES
            );

        // if we had the COMMANDLIST_HEADER_PREFIX 'c' situation and the shorter name did not match, but the longer did
        // be sure to include that 1 extra character in the token
        if (match_idx && match_idx == 1 && is_prefix_search) {
            mark_end(lexer);
        }

        reset(scanner);
        return match_idx;
    }

    reset(scanner);
    return false;
}

static inline bool scan_section_header_lookahead(Scanner *scanner, TSLexer *lexer, const bool *valid_symbols) {
    // fprintf(stderr, "[Lykare]: lookahead for section header\n");
    bool saw_text = false;
    bool is_guard = (valid_symbols[SECTION_HEADER_GUARD] && !valid_symbols[SECTION_HEADER_START]);
    bool both_valid = (valid_symbols[SECTION_HEADER_GUARD] && valid_symbols[SECTION_HEADER_START]);

    // if (both_valid) fprintf(stderr, "[Lykare]: both symbols valid...\n");

    // fprintf(stderr, "[Lykare]: eating consecutive newlines if present\n");
    while (lexer->lookahead == '\r' || lexer->lookahead == '\n') {
        // fprintf(stderr, "[Lykare]: starting loop iteration\n");
        skip(lexer);
    }

    for (;;) {
        // fprintf(stderr, "[Lykare]: starting loop iteration\n");
        char lookahead = lexer->lookahead;
        bool is_wspace = iswspace(lookahead);

        if (lookahead == '\n' || is_eof(lexer)) {
            // fprintf(stderr, "[Lykare]: newline or EOF ahead\n");
            if (!saw_text) {
                // fprintf(stderr, "[Lykare]: did not see text\n");
                if (is_guard) {
                    // fprintf(stderr, "[Lykare]: is guard, return true unless EOF\n");
                    lexer->result_symbol = SECTION_HEADER_GUARD;
                    return !is_eof(lexer);
                }

                if (is_eof(lexer)) {
                    lexer->result_symbol = SECTION_HEADER_START;
                    return false;
                }

                // fprintf(stderr, "[Lykare]: was looking for start, break loop\n");
                break;
            }
            else {
                // unreachable
                // fprintf(stderr, "[Lykare]: reached unreachable\n");
                return false;
            }
        }
        else if (!saw_text && lookahead == ';') {
            // fprintf(stderr, "[Lykare]: found a comment line\n");
            lexer->result_symbol = SECTION_HEADER_START;
            return false;
        }
        else if (!saw_text && lookahead == '[') {
            // fprintf(stderr, "[Lykare]: potential section start ahead\n");
            mark_end(lexer);

            if (scan_maybe_section_header(scanner, lexer, valid_symbols, false)) {
                // fprintf(stderr, "[Lykare]: found section start\n");
                if (is_guard) {
                    // fprintf(stderr, "[Lykare]: is guard, so return false\n");
                    lexer->result_symbol = SECTION_HEADER_GUARD;
                    return false;
                }

                // fprintf(stderr, "[Lykare]: was looking for start, so return true\n");
                lexer->result_symbol = SECTION_HEADER_START;
                return true;
            }
        }
        else {
            if (!is_wspace && !saw_text) {
                saw_text = true;
            }

            if (saw_text && is_guard) {
                // fprintf(stderr, "[Lykare]: saw text that wasn't [ or ; during guard search, return true\n");
                lexer->result_symbol = SECTION_HEADER_GUARD;
                return true;
            }
            else if (saw_text) {
                // fprintf(stderr, "[Lykare]: saw text that wasn't [ or ; during start search\n");
                break;
            }
            else {
                // fprintf(stderr, "[Lykare]: skip all non-newline ws\n");
                skip(lexer);
            }
        }
    }

    if (both_valid) {
        lexer->result_symbol = SECTION_HEADER_GUARD;
        return true;
    }
    else {
        lexer->result_symbol = SECTION_HEADER_START;
        return false;
    }
}

static inline bool scan_not_pooled_variable_lookahead(Scanner *scanner, TSLexer *lexer) {
    // fprintf(stderr, "[Lykare]: lookahead for 'pool' after a '$'\n");

    char *target_term = "pool";
    char lookahead = lexer->lookahead;
    bool res = true;

    lexer->result_symbol = POOLED_VARIABLE_GUARD;
    mark_end(lexer);


    if (lookahead != 'p' && lookahead != 'P') {
        return res;
    }

    do {
        array_push(&scanner->word, lookahead);
        consume(lexer);

        lookahead = lexer->lookahead;
    } while (scanner->word.size < 4);

    array_push(&scanner->word, '\0'); // NUL term char array before comparison

    if (!strcmp(strlwr(scanner->word.contents), target_term)) {
        // fprintf(stderr, "[Lykare]: first 4 chars matched 'pool', checking for index expression\n");

        bool tmp = false;

        for (;;) {
            if (lookahead == '[') {
                res = false;
                break;
            }

            if (iswspace(lookahead) || is_eof(lexer)) {
                break;
            }

            switch (lookahead) {
            case '$':
            case '=':
            case '+':
            case '*':
            case '/':
            case '&':
            case '|':
            case '^':
            case '(':
            case ')':
            case '-':
            case '!':
                tmp = true;
                break;
            default:
                break;
            }

            if (tmp) {
                break;
            }

            consume(lexer);
            lookahead = lexer->lookahead;
        }
    }

    reset(scanner);
    return res;
}

static inline bool scan_line(Scanner *scanner, TSLexer *lexer, const bool *valid_symbols) {
    // fprintf(stderr, "[Lykare]: lookahead for external line\n");
    bool saw_text = false;

    // fprintf(stderr, "[Lykare]: eating consecutive newlines if present\n");
    while (lexer->lookahead == '\r' || lexer->lookahead == '\n') {
        // fprintf(stderr, "[Lykare]: starting loop iteration\n");
        skip(lexer);
    }

    for (;;) {
        // fprintf(stderr, "[Lykare]: starting loop iteration\n");
        char lookahead = lexer->lookahead;
        bool is_wspace = iswspace(lookahead);

        if (lookahead == '\n' || is_eof(lexer)) {
            // fprintf(stderr, "[Lykare]: saw newline or EOF\n");
            if (saw_text) {
                // fprintf(stderr, "[Lykare]: saw text before, so eat the newline, mark end, return true\n");
                if (!is_eof(lexer)) consume(lexer);
                mark_end(lexer);
                lexer->result_symbol = EXTERNAL_LINE;
                return true;
            }
            else {
                // fprintf(stderr, "[Lykare]: didn't see text, return false for external line\n");
                lexer->result_symbol = EXTERNAL_LINE;
                return false;
            }
        }
        else if (!saw_text && lookahead == ';') {
            // fprintf(stderr, "[Lykare]: did not see text yet, ; ahead, must be comment so return false\n");
            lexer->result_symbol = EXTERNAL_LINE;
            return false;
        }
        else if (!saw_text && lookahead == '[') {
            // fprintf(stderr, "[Lykare]: did not see text yet, see [ ahead, if section header, return false\n");
            if (scan_maybe_section_header(scanner, lexer, valid_symbols, false)) {
                lexer->result_symbol = EXTERNAL_LINE;
                return false;
            }

            // fprintf(stderr, "[Lykare]: was not a section header ahead, set saw text to true\n");
            saw_text = true;
        }
        else {
            if (!is_wspace && !saw_text) {
                saw_text = true;
            }

            if (saw_text) {
                // fprintf(stderr, "[Lykare]: saw text, so consume all chars include non-newline/EOF ws\n");
                consume(lexer);
            }
            else {
                // fprintf(stderr, "[Lykare]: skip non-newline/EOF ws\n");
                skip(lexer);
            }
        }
    }

    // shouldn't get here but just in case
    // fprintf(stderr, "[Lykare]: should be unreachable\n");
    lexer->result_symbol = EXTERNAL_LINE;
    return false;
}

static inline bool scan_suffixed_section_header(TSLexer *lexer, TokenType current_symbol , const bool *valid_symbols) {
    bool saw_text = false, on_ws = true, is_start = true, not_error = (current_symbol != ERROR_SENTINEL);

    for (;;) {
        if (lexer->lookahead == ']' || lexer->lookahead == '\r' || lexer->lookahead == '\n' || is_eof(lexer)) {
            if (not_error) lexer->result_symbol = current_symbol;

            // if we see a terminal but have not seen any text yet
            // only the Include header can be valid, as it is the only one allowed a null suffix
            if (!saw_text) {
                return (not_error && valid_symbols[SUFFIXED_INCLUDE_HEADER]);
            }

            if (!on_ws) mark_end(lexer);

            return not_error; // return captured text excluding the trailing whitespace
        }
        else if (iswspace(lexer->lookahead)) {
            if (is_start) is_start = false;
            if (!on_ws) {
                on_ws = true;
                if (saw_text) {
                    mark_end(lexer);
                }
            }
            consume(lexer);
        }
        else {
            if (on_ws) {
                on_ws = false;
                if (!saw_text) {
                    saw_text = true;
                    if (is_start) {
                        consume(lexer);
                        is_start = false;
                        continue;
                    }
                    mark_end(lexer); // mark all leading whitespace as part of the token
                }
            }
            consume(lexer);
        }
    }
}

static inline bool scan_custom_resource_identifier(TSLexer *lexer) {
    // fprintf(stderr, "[Lykare]: checking for a custom resource or pool identifier\n");
    lexer->result_symbol = CUSTOM_RESOURCE_IDENTIFIER;

    char lookahead;
    bool is_start = true, is_terminal_ahead = false, is_special_terminal = false;

    // fprintf(stderr, "[Lykare]: starting loop iteration\n");
    while (!is_terminal_ahead) {
        lookahead = lexer->lookahead;
        is_terminal_ahead = is_eof(lexer) || iswspace(lookahead);

        if (is_terminal_ahead) {
            break;
        }

        if (is_start) {
            is_start = false;
        }

        // The following intentionally deviates from 3dm allowances
        // by disallowing some characters in a resource name which 3dm allows
        // 3dm may use whitespace for disambiguation, but I don't agree with
        // mixing whitespace significance in some places but not in others
        switch (lookahead) {
        case '[':
        case '$':
        case '=':
        case '+':
        case '*':
        case '/':
        case '&':
        case '|':
        case '^':
        case '(':
        case ')':
        case ',':
            is_terminal_ahead = true;
            break;
        case '!':
            mark_end(lexer);
            consume(lexer); // consume '!'
            if (lexer->lookahead == '=') {
                is_terminal_ahead = true;
                is_special_terminal = true;
                break;
            }
            else {
                continue;
            }
        case '-':
            mark_end(lexer);
            consume(lexer); // consume '-'
            if (lexer->lookahead == '>') {
                is_terminal_ahead = true;
                is_special_terminal = true;
                break;
            }
            else {
                continue;
            }
        default:
            consume(lexer);
            break;
        }
    }

    if (!is_start) {
        if (!is_special_terminal) mark_end(lexer);
        return true;
    }

    return false;
}

static inline bool scan_for_regex_suffix(Scanner *scanner, TSLexer *lexer, const bool *valid_symbols) {
    SearchState ss = NOTSEARCHING;
    TokenType result;
    char *search_target;

    for (;;) {
        if (is_eof(lexer)) {
            mark_end(lexer);
            lexer->result_symbol = REGEX_COMMANDLIST_HEADER;
            reset(scanner);
            return true;
        }
        switch (ss)
        {
        case GENSEARCH:
            switch (lexer->lookahead)
            {
            case 'I':
            case 'i':
                ss = INSSEARCH;
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer);
                break;
            case 'P':
            case 'p':
                ss = PATSEARCH;
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer);
                break;
            default:
                ss = NOTSEARCHING;
                consume(lexer);
                break;
            }
            break;
        case INSSEARCH:
        case REPSEARCH:
            if (ss & 0x2) {
                search_target = "replace";
                result = REGEX_REPLACE_HEADER;
            }
            else {
                search_target = "insertdeclarations";
                result = REGEX_DECLARATIONS_HEADER;
            }

            switch (lexer->lookahead)
            {
            case ']':
            case '\n':
                mark_end(lexer);

                array_push(&scanner->word, '\0'); // terminate the accumulated word

                strlwr(scanner->word.contents);

                if (!strcmp(scanner->word.contents, search_target)) {
                    reset(scanner);
                    if (!valid_symbols[result]) return false;
                    lexer->result_symbol = result;
                    return true;
                }

                ss = NOTSEARCHING;
                reset(scanner);
                break;
            case '.':
                mark_end(lexer);
                ss = NOTSEARCHING;
                reset(scanner);
                break;
            default:
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer);
                break;
            }
            break;
        case PATSEARCH:
            switch (lexer->lookahead)
            {
            case '.':
            case ']':
            case '\n':
                mark_end(lexer);

                array_push(&scanner->word, '\0'); // terminate the accumulated word

                strlwr(scanner->word.contents);

                if (!strcmp(scanner->word.contents, "pattern")) {
                    if (lexer->lookahead == '.') {
                        ss = REPSEARCH;
                        reset(scanner);
                        consume(lexer);
                    }
                    else {
                        reset(scanner);
                        if (!valid_symbols[REGEX_PATTERN_HEADER]) {
                            return false;
                        }
                        lexer->result_symbol = REGEX_PATTERN_HEADER;
                        return true;
                    }
                }
                else {
                    lexer->result_symbol = REGEX_COMMANDLIST_HEADER;
                    ss = NOTSEARCHING;
                    reset(scanner);
                }
                break;
            default:
                array_push(&scanner->word, lexer->lookahead);
                consume(lexer);
                break;
            }
            break;
        case NOTSEARCHING:
        default:
            switch (lexer->lookahead)
            {
            case ']':
            case '\n':
                mark_end(lexer);
                lexer->result_symbol = REGEX_COMMANDLIST_HEADER;
                return true;
            case '.':
                mark_end(lexer);
                ss = GENSEARCH;
                consume(lexer);
                break;
            default:
                consume(lexer);
                break;
            }
            break;
        }
    }
}

bool tree_sitter_migoto_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    if (valid_symbols[ERROR_SENTINEL]) {
        // fprintf(stderr, "[Lykare]: error\n");
        return false;
    }

    if (valid_symbols[NEWLINE]) {
        // fprintf(stderr, "[Lykare]: newline\n");
        return scan_end_of_line(lexer);
    }

    // From tree-sitter-zig github PR#10, under MIT license
    if (valid_symbols[DOC_COMMENT_CONTENT]) {
        // fprintf(stderr, "[Lykare]: doc comment\n");
        lexer->result_symbol = DOC_COMMENT_CONTENT;
        while (true) {
            if (is_eof(lexer)) {
                return true;
            }

            if (lexer->lookahead == '\n') {
                // including the line ending in doc
                // comments is necessary for markdown injections
                consume(lexer);
                return true;
            }

            consume(lexer);
        }
    }

    if (valid_symbols[NAMESPACE_RESOLUTION_START]) {
        // fprintf(stderr, "[Lykare]: namespace start\n");
        bool tmp = scan_namespace_res_start_end(lexer, NAMESPACE_RESOLUTION_START);
        if (!tmp && valid_symbols[CUSTOM_RESOURCE_IDENTIFIER]) {
            // fprintf(stderr, "[Lykare]: Custom Resource Identifier\n");
            return scan_custom_resource_identifier(lexer);
        }
        return tmp;
    }

    if (valid_symbols[NAMESPACE_RESOLUTION_CONTENT]) {
        // fprintf(stderr, "[Lykare]: namespace content\n");
        return scan_namespace_res_content(lexer);
    }

    if (valid_symbols[NAMESPACE_RESOLUTION_END]) {
        // fprintf(stderr, "[Lykare]: namespace end\n");
        return scan_namespace_res_start_end(lexer, NAMESPACE_RESOLUTION_END);
    }

    if (valid_symbols[CUSTOM_RESOURCE_IDENTIFIER]) {
        // fprintf(stderr, "[Lykare]: Custom Resource Identifier\n");
        return scan_custom_resource_identifier(lexer);
    }

    if (valid_symbols[SUFFIXED_KEY_HEADER] || valid_symbols[SUFFIXED_PRESET_HEADER] ||
        valid_symbols[SUFFIXED_RESOURCE_HEADER] || valid_symbols[SUFFIXED_INCLUDE_HEADER] ||
        valid_symbols[SUFFIXED_COMMANDLIST_HEADER])
    {
        // fprintf(stderr, "[Lykare]: suffixes\n");
        TokenType t = ERROR_SENTINEL;

        if (valid_symbols[SUFFIXED_KEY_HEADER])
            t = SUFFIXED_KEY_HEADER;
        else if (valid_symbols[SUFFIXED_PRESET_HEADER])
            t = SUFFIXED_PRESET_HEADER;
        else if (valid_symbols[SUFFIXED_RESOURCE_HEADER])
            t = SUFFIXED_RESOURCE_HEADER;
        else if (valid_symbols[SUFFIXED_INCLUDE_HEADER])
            t = SUFFIXED_INCLUDE_HEADER;
        else if (valid_symbols[SUFFIXED_COMMANDLIST_HEADER])
            t = SUFFIXED_COMMANDLIST_HEADER;

        return scan_suffixed_section_header(lexer, t, valid_symbols);
    }

    Scanner *scanner = (Scanner*) payload;

    if (valid_symbols[POOLED_VARIABLE_GUARD]) {
        // fprintf(stderr, "[Lykare]: guard to look for start of pooled var\n");

        // Did we see 'pool' as the next 4 chars?
        bool tmp = !scan_not_pooled_variable_lookahead(scanner, lexer);

        if (tmp && valid_symbols[POOLED_VARIABLE_START]) {
            lexer->result_symbol = POOLED_VARIABLE_START;
            return true;
        }
        else if (tmp && !valid_symbols[POOLED_VARIABLE_START]) {
            return false;
        }
        else {
            return true;
        }
    }

    bool is_prefix_search = (
        valid_symbols[KEY_HEADER_PREFIX] || valid_symbols[PRESET_HEADER_PREFIX] ||
        valid_symbols[INCLUDE_HEADER_PREFIX] || valid_symbols[COMMANDLIST_HEADER_PREFIX] ||
        valid_symbols[COMMANDLIST_CALLABLE_PREFIX] || valid_symbols[CUSTOMSHADER_CALLABLE_PREFIX] ||
        valid_symbols[CUSTOMRESOURCE_HEADER_PREFIX] || valid_symbols[RESOURCEPOOL_HEADER_PREFIX]
    );

    if (is_prefix_search) {
        // fprintf(stderr, "[Lykare]: Header Prefix Search, not Regex\n");
        return scan_section_header_prefix(scanner, lexer, valid_symbols, is_prefix_search);
    }

    if (valid_symbols[REGEX_COMMANDLIST_HEADER] || valid_symbols[REGEX_DECLARATIONS_HEADER] ||
        valid_symbols[REGEX_PATTERN_HEADER] || valid_symbols[REGEX_REPLACE_HEADER])
    {
        // fprintf(stderr, "[Lykare]: Header Prefix Search, Regex\n");
        return scan_for_regex_suffix(scanner, lexer, valid_symbols);
    }

    if (valid_symbols[EXTERNAL_LINE] && !(valid_symbols[SECTION_HEADER_START] || valid_symbols[SECTION_HEADER_GUARD])) {
        // fprintf(stderr, "[Lykare]: external line, section start, or guard\n");
        return scan_line(scanner, lexer, valid_symbols);
    }

    if (valid_symbols[SECTION_HEADER_START] || valid_symbols[SECTION_HEADER_GUARD]) {
        // fprintf(stderr, "[Lykare]: section start or guard\n");
        return scan_section_header_lookahead(scanner, lexer, valid_symbols);
    }

    if (valid_symbols[ANTIWHITESPACE]) {
        // fprintf(stderr, "[Lykare]: antiwhitespace\n");
        lexer->result_symbol = ANTIWHITESPACE;
        return !iswspace(lexer->lookahead);
    }

    // fprintf(stderr, "[Lykare]: how??\n");

    return false;
}
