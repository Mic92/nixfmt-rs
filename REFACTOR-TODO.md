# Parser Refactoring TODO

## Code Duplication and Sharing Opportunities

### 1. Context Parameter Parsing (Multiple Locations)
**Lines:** 108-131, 184-198, 226-245, 287-302, 520-529, 553-561
**Pattern:** Repeated logic for parsing context parameters (pattern @ pattern : body)
**Opportunity:** Extract common helper function

### 2. Set Parameter Parsing After Set Construction (Multiple Locations)
**Lines:** 172-182 (empty set), 216-225 (with attrs), 278-286 (ellipsis)
**Pattern:** After parsing set parameter, check for `:` or `@`, then parse body
**Opportunity:** Extract helper to reduce duplication

### 3. ✅ Take Token + Advance Pattern (COMPLETED)
**Lines:** Throughout - examples at 94-95, 109-110, 185-186, 228-229, etc.
**Pattern:** `let x = self.take_current(); self.advance()?;`
**Action Taken:** Extracted `take_and_advance()` helper method (see completed simplifications)

### 4. ✅ Lexer Save/Restore State Pattern (COMPLETED)
**Lines:** 150-151, 257-258, 1351-1352, 1381-1382
**Pattern:** Save lexer state and current token, then optionally restore
**Action Taken:** Created `ParserState` struct and `save_state()`/`restore_state()` helpers (see completed simplifications)

### 5. ✅ String/Path Character Validation (COMPLETED)
**Lines:** 1223-1224 (is_scheme_char), 1228-1249 (is_uri_char)
**Pattern:** Similar character validation logic
**Action Taken:** Extracted `URI_SCHEME_SPECIAL_CHARS` and `URI_SPECIAL_CHARS` constants (see completed simplifications)

### 6. Escape Sequence Handling
**Lines:** 1494-1517 (simple string), 1670-1702 (indented string)
**Pattern:** Similar escape sequence parsing with slight differences
**Opportunity:** Possibly extract common logic

### 7. Interpolation Parsing
**Lines:** 1541-1575 (string interpolation), 1896-1913 (selector interpolation)
**Pattern:** Parse ${expr} with slight differences
**Opportunity:** Unify or clarify differences

### 8. Parse Keyword Expressions (Let/If/With/Assert)
**Lines:** 626-682
**Pattern:** All follow similar structure: keyword + parts + parse_expression
**Opportunity:** These are already fairly clean, minimal abstraction needed

### 9. ✅ Empty Trivia Checks (COMPLETED)
**Lines:** 920-922, 928-932, 1791-1793, 1800-1802
**Pattern:** Check if pre_trivia is empty, extract as Comments item
**Action Taken:** Extracted `collect_trivia_as_comments()` helper method (see completed simplifications)

## Parser Completed Simplifications

### Improvements Made (2025-10-26):

#### Phase 1: Helper Function Extraction

1. ✅ **`take_and_advance()` helper** - Extracted common "take token + advance" pattern
   - Used in ~25+ locations throughout parser
   - Examples: lines 94, 98, 107, 150, 171, 180, 212, 221, 271, 280, 353, 507, 511, 542, 562, 571, 584, 593, 673, 679, 701
   - Reduced code duplication significantly
   - Makes the common pattern more explicit and easier to maintain

2. ✅ **`collect_trivia_as_comments()` helper** - Extracted trivia collection pattern
   - Used in `parse_binders()` (lines 892, 899)
   - Used in `parse_list_items()` (lines 1759, 1767)
   - Reduces duplication of the "check pre_trivia, take if not empty" pattern
   - 4 instances consolidated into 2 call sites with 1 helper

3. ✅ **`maybe_parse_binary_operation()` helper** - Extracted conditional binary operation parsing
   - Used in `continue_operation_from()` (line 331)
   - Used in `parse_operation_or_lambda()` (line 378)
   - Eliminates repeated "if is_binary_op() { parse_binary_operation } else { Ok(expr) }" pattern

#### Phase 2: Code Simplification

4. ✅ **Removed `parse_term_expr()` function** - Inlined trivial wrapper
   - Function was just wrapping `parse_term()` in `Expression::Term()`
   - Inlined at 3 call sites (lines 331, 688, 693)
   - Removed 4 lines of unnecessary abstraction
   - Makes code more direct and easier to follow

5. ✅ **Simplified `continue_operation_from()`** - Reduced branching complexity
   - Restructured to use single `maybe_parse_binary_operation()` call at end
   - Eliminated 3 instances of repeated binary operation checking
   - Reduced function from 33 lines to 22 lines
   - Clearer control flow: build expression, then check for binary op once

6. ✅ **Simplified `is_empty_line()`** - More idiomatic pattern matching
   - Changed from nested match to simpler boolean expression
   - From 6 lines to 2 lines
   - More idiomatic Rust style

7. ✅ **Simplified `fix_first_line()`** - Used if-let pattern
   - Changed from manual indexing to safer `first().cloned()`
   - More idiomatic Rust with explicit Option handling
   - Same functionality, clearer intent

8. ✅ **Extracted URI character validation constants** - Improved clarity and maintainability
   - Added `URI_SCHEME_SPECIAL_CHARS` constant for scheme characters
   - Added `URI_SPECIAL_CHARS` constant for URI body characters
   - Updated `is_scheme_char()` and `is_uri_char()` to use constants
   - Makes allowed characters explicit and easier to modify
   - Documented in code comments based on nixfmt spec

9. ✅ **Added parser state checkpoint helpers** - Cleaner save/restore API
   - Created `ParserState` struct to bundle lexer state and current token
   - Added `save_state()` helper method
   - Added `restore_state()` helper method
   - Applied to 3 locations (lines ~163, ~1300, ~1330)
   - Eliminates manual tracking of separate state components
   - More maintainable and less error-prone

### Total Impact:
- **Helper functions added**: 5 new focused helpers (take_and_advance, collect_trivia_as_comments, maybe_parse_binary_operation, save_state, restore_state)
- **Functions removed**: 1 trivial wrapper eliminated (parse_term_expr)
- **Constants extracted**: 2 character validation constants for URIs
- **Lines saved**: ~50-60 lines of duplicated code removed
- **Improved patterns**:
  - Consolidated 25+ instances of take+advance
  - Unified 4 instances of trivia collection
  - Simplified 3 instances of state save/restore
  - Centralized URI character definitions
- **Consistency**: All common patterns now use dedicated helpers
- **Readability**: Clearer control flow in operation parsing and backtracking
- **Maintainability**: Changes to common patterns now centralized
- **Safety**: State save/restore bundled to prevent mismatches
- **Test coverage**: All 37 parser tests still passing

### Not Changed (and why):
- Complex parameter parsing functions - Already have clear structure, extraction would obscure logic
- Keyword expression parsers (let/if/with/assert) - Already clean and simple
- Binary operation precedence logic - Complex but necessary, already well-structured
- String parsing functions - Complex state management, already optimized
- Context parameter patterns - While duplicated, each instance has subtle differences

## Parser Remaining Complex Functions

### High Complexity (Should be broken down)
1. `parse_set_parameter_or_literal` (lines 148-319) - 171 lines, multiple decision points
2. `parse_abstraction_or_operation` (lines 70-145) - 75 lines, complex branching
3. `parse_binary_operation` (lines 799-865) - 66 lines with special TPlus hack
4. `parse_path` (lines 1113-1219) - 106 lines, complex state management
5. `parse_postfix_selection` (lines 1346-1415) - 69 lines, state save/restore

### Medium Complexity (Could benefit from simplification)
1. `parse_param_attrs` (lines 569-623) - 54 lines, error detection logic
2. `items_to_param_attrs` (lines 457-508) - 51 lines, conversion logic
3. `parse_operation_or_lambda` (lines 356-405) - 49 lines, lookahead logic

### Already Simple (No changes needed)
1. Keyword expression parsers (let/if/with/assert) - clean and clear
2. Predicate methods (is_term_start, is_binary_op, etc.) - appropriate
3. Simple term parsers (identifier, integer, float) - trivial

## Parser Notes

- File has 2465 lines total
- Many long functions due to parser state management
- Some duplication is inherent to recursive descent parsing
- Focus on extracting truly repeated logic, not one-off patterns
