# Nixfmt Auto-Formatting Research

## Overview

Nixfmt uses a **Wadler-Leijen pretty-printing** architecture with a **greedy layout algorithm**. The formatting pipeline is:

```
Nix Source → Lexer → Parser → AST → Pretty (Doc IR) → Fixup → Layout → Text Output
```

## Architecture Components

### 1. Doc IR (Intermediate Representation)

**Location**: `Predoc.hs` / `predoc.rs`

The Doc IR is the key abstraction that separates formatting logic from rendering:

```haskell
type Doc = [DocE]

data DocE
  = Text Int Int TextAnn Text  -- nesting depth, offset, annotation, text
  | Spacing Spacing
  | Group GroupAnn Doc
```

#### Spacing Types

Sequential spacings merge to maximum (e.g., Space + Emptyline = Emptyline):

```haskell
data Spacing
  = Softbreak   -- Line break or nothing (soft)
  | Break       -- Line break or nothing
  | Hardspace   -- Always a space
  | Softspace   -- Line break or space (soft)
  | Space       -- Line break or space
  | Hardline    -- Always a line break
  | Emptyline   -- Two line breaks
  | Newlines Int -- n line breaks
```

**Key distinctions:**
- **Soft** (`Softbreak`, `Softspace`): Only expand when necessary for line width
- **Hard** (`Hardspace`, `Hardline`, `Emptyline`): Never change
- **Flexible** (`Break`, `Space`): Expand when group expands

#### Group Annotations

```haskell
data GroupAnn
  = RegularG     -- Standard group
  | Priority     -- Expand this first when parent doesn't fit
  | Transparent  -- Pass-through for priority group handling
```

**Priority groups** are the key innovation:
- When a group doesn't fit on one line, try expanding priority subgroups first
- Multiple priority groups are tried in **reverse order** (last first)
- This allows the last argument to be multiline without forcing earlier args to be multiline

#### Text Annotations

```haskell
data TextAnn
  = RegularT         -- Normal text
  | Comment          -- Comment (doesn't count toward line length)
  | TrailingComment  -- Single-line trailing comment
  | Trailing         -- Only rendered in expanded groups
```

### 2. Pretty Typeclass

**Location**: `Predoc.hs` + `Pretty.hs`

```haskell
class Pretty a where
  pretty :: a -> Doc
```

Every AST node implements `Pretty` to convert to Doc IR.

#### Core Combinators

```haskell
-- Text builders
text :: Text -> Doc                  -- Regular text
comment :: Text -> Doc               -- Comment text
trailingComment :: Text -> Doc       -- Trailing comment
trailing :: Text -> Doc              -- Text only in expanded groups

-- Grouping
group :: (Pretty a) => a -> Doc      -- Create a group
group' :: GroupAnn -> a -> Doc       -- Create annotated group
nest :: (Pretty a) => a -> Doc       -- Increase nesting depth
offset :: Int -> a -> Doc            -- Manual offset (for indented strings)

-- Spacing
line :: Doc          -- Line break or space
line' :: Doc         -- Line break or nothing
softline :: Doc      -- Line break or space (soft)
softline' :: Doc     -- Line break or nothing (soft)
hardspace :: Doc     -- Always space
hardline :: Doc      -- Always line break
emptyline :: Doc     -- Two line breaks
newline :: Doc       -- Single newline

-- Combinators
sepBy :: Doc -> [a] -> Doc           -- Separate with delimiter
surroundWith :: Doc -> a -> Doc      -- Surround with delimiter
hcat :: [a] -> Doc                   -- Horizontal concatenation
```

### 3. Fixup Pass

**Function**: `fixup :: Doc -> Doc`

Preprocesses Doc IR before layout:

1. **Merges consecutive spacing elements**
2. **Moves hard spacing out of groups** (especially comments)
   - Critical: Prevents hardlines in comments from wrongly expanding groups
3. **Removes empty groups**
4. **Merges consecutive text elements**

Example:
```haskell
-- Before fixup:
Group [Text "foo", Spacing Hardline, Comment "# bar", Spacing Hardline, Text "baz"]

-- After fixup:
[Comment "# bar", Spacing Hardline, Group [Text "foo", Text "baz"]]
```

### 4. Layout Algorithm

**Function**: `layoutGreedy :: Int -> Int -> Doc -> Text`

Greedy line-breaking with priority group expansion:

#### State Tracking

```haskell
type St = (Int, NonEmpty (Int, Int))
-- (currentColumn, stack of (currentIndent, nestingLevel))
```

#### Algorithm Steps

1. **For each Group**: Try to fit on one line using `fits`
2. **If fails**: Check for priority groups within
3. **Priority group expansion**:
   - Try: `pre (compact) + prio (expanded) + post (compact)`
   - Test each priority group in **reverse order**
4. **If all fail**: Fully expand the group

#### Key Helper Functions

**`fits :: Int -> Int -> Doc -> Maybe Text`**
- Checks if doc fits in given width
- Returns rendered text or Nothing
- Comments don't count toward width
- Trailing comments get special treatment

**`unexpandSpacing' :: Maybe Int -> Doc -> Maybe Doc`**
- Forces compact layout on a doc
- Fails if contains hardlines or exceeds length
- Recursively processes inner groups

**`firstLineFits :: Int -> Int -> Doc -> Bool`**
- Checks if first line fits target width
- Used for soft spacing resolution

#### Soft Spacing Resolution

During layout, soft spacings are resolved:
- `Softspace` → space if next content fits, otherwise newline
- `Softbreak` → nothing if next content fits, otherwise newline

## Formatting Patterns

### Function Application

**Pattern**: `pre f line a post`

```
f g a    → pre [f line g] line a post
f g h a  → pre [[f line g] line h] line a post
```

**Tricks**:
1. Each function call is grouped → greedy expansion
2. Arguments are **Priority groups** → expand last first
3. Callers inject `pre`/`post` for context-aware formatting

**Special cases**:
- Two consecutive list arguments: treat together
- Selections: use Transparent groups
- Simple expressions: force compact if possible

### Attribute Sets

```haskell
prettySet :: Bool -> (Maybe Leaf, Leaf, Items Binder, Leaf) -> Doc
```

- Empty sets: preserve line breaks from source
- Singleton sets: allow one-line
- Multi-item sets: always expand (unless `wide=False`)
- `wide` parameter for extra-aggressive expansion

### Lists

- Empty lists: preserve source line breaks
- Simple items (≤6, all simple): allow `sepBy line` (not `hardline`)
- Others: expand with `line` separator

### Strings

**Simple strings** (`"..."`)
```haskell
prettySimpleString :: [[StringPart]] -> Doc
prettySimpleString parts =
  group $
    text "\""
      <> sepBy (text "\n") (map pretty parts)
      <> text "\""
```

**Indented strings** (`''...''`)
- Multi-line: insert `line'` after opening `''`
- Single-line: omit the initial line break
- Interpolations: force compact if ≤30 chars

### Let Bindings

```haskell
pretty (Let let_ binders Ann{preTrivia, value = in_, trailComment} expr) =
  letPart <> hardline <> inPart
  where
    letPart = group $ pretty let_ <> hardline <> letBody
    letBody = nest $ prettyItems binders
    inPart = group $
      pretty in_
        <> hardline
        <> pretty (preTrivia ++ convertTrailing trailComment)
        <> pretty expr
```

- Always fully expanded (no single-line form)
- Comments around `in` are moved to body

### If-Then-Else

```haskell
prettyIf :: Doc -> Expression -> Doc
prettyIf sep (If if_ cond then_ expr0 else_ expr1) =
  group (pretty if_ <> line <> nest (pretty cond) <> line <> pretty then_)
    <> surroundWith sep (nest $ group expr0)
    <> pretty (moveTrailingCommentUp else_)
    <> hardspace
    <> prettyIf hardline expr1
```

- `if cond then` on one line if fits
- Nested `else if` handled recursively

### Lambdas (Abstractions)

**Simple parameter**:
```haskell
pretty (Abstraction (IDParameter param) colon body) =
  group' RegularG $ line' <> pretty param <> pretty colon <> absorbAbs 1 body
```

- Multiple ID parameters absorbed: `x: y: z: body`
- Absorbable body: starts on same line
- Non-absorbable with >2 params: force new line

**Set parameter**:
```haskell
pretty (SetParameter bopen attrs bclose) =
  group $
    pretty (moveTrailingCommentUp bopen)
      <> surroundWith sep (nest $ sepBy sep $ handleTrailingComma ...)
      <> pretty bclose
```

- Trailing comma on last element only in expanded form
- Comments moved from comma to parameter name

## Comment Handling

### Comment Preservation

Comments are attached to AST nodes as trivia:

1. **Leading comments** → `preTrivia` field of next token
2. **Trailing comments** → `trailComment` field of same-line token
3. **Block comments** → Stored as multi-line trivia

### Comment Movement Functions

**`moveTrailingCommentUp :: Ann a -> Ann a`**
- Converts trailing comment to leading comment
- Used before keywords (else, in, etc.)

**`moveParamsComments :: [ParamAttr] -> [ParamAttr]`**
- Moves comments in parameter lists
- Handles leading vs trailing comma style

### Special Handling

**Trailing comment idempotency fix**:
```haskell
-- [ # comment
--   1
-- ]
-- Would reparse as:
-- [
--   # comment
--   1
-- ]
-- Fix: shift comment by one space:
-- [  # comment
--   1
-- ]
```

## Absorbable Patterns

### `isAbsorbableExpr :: Expression -> Bool`

Expressions that can be kept on same line as `=`:

- Lists, sets, strings
- `with expr` where expr is absorbable
- Simple lambdas with absorbable bodies

### `isSimple :: Expression -> Bool`

Expressions with no internal complexity:

- Simple literals and identifiers
- Parenthesized simple expressions
- Selections without `or` default
- Function applications (max depth 2)

**Critical**: Must have no leading comments (LoneAnn check)

### `absorbRHS :: Expression -> Doc`

Decides whether to break to new line after `=`:

```haskell
absorbRHS expr = case expr of
  _ | isAbsorbableExpr expr ->
      nest $ hardspace <> group (absorbExpr True expr)
  (Application f a) ->
      nest $ prettyApp False line False f a
  _ ->
      nest $ line <> group' RegularG expr
```

## Smart Indentation

**Multiple nesting levels on one line → single indent bump**

```haskell
nest :: (Pretty a) => a -> Doc
nest x = map go $ pretty x
  where
    go (Text i o ann t) = Text (i + 1) o ann t
    go (Group ann inner) = Group ann (map go inner)
    go spacing = spacing
```

During layout:
```haskell
putText nl off t = ...
  case textNL `compare` nl of
    -- Only increase indent if first on line
    GT -> putR ((if cc == 0 then ci + iw else ci, textNL) <| indents) >> go'
```

This prevents excessive indentation like:
```
{
      foo = {
              bar = 1;
            };
}
```

Instead:
```
{
  foo = {
    bar = 1;
  };
}
```

## Implementation Strategy for Rust

### Phase 1: Port Predoc Module

1. Create `src/predoc.rs` with Doc types
2. Implement spacing types and merging
3. Implement group annotations
4. Implement basic combinators (text, comment, line, etc.)

### Phase 2: Implement Fixup

1. Port `fixup :: Doc -> Doc`
2. Port `mergeSpacings`
3. Port comment extraction logic
4. Add tests comparing with Haskell output

### Phase 3: Implement Layout Algorithm

1. Port state tracking (`St` type)
2. Implement `fits` function
3. Implement `firstLineFits`
4. Implement `unexpandSpacing'`
5. Implement greedy layout with priority groups
6. Add comprehensive tests

### Phase 4: Implement Pretty Instances

1. Start with simple types (Token, Trivium)
2. Implement terms (literals, strings, paths)
3. Implement complex expressions (let, if, lambda)
4. Implement operators and applications
5. Test each instance against Haskell `--ir` output

### Phase 5: Integration

1. Wire up in lib.rs: `parse → pretty → fixup → layout`
2. Add `--ir` flag to CLI
3. Run full test suite
4. Compare with Haskell nixfmt byte-for-byte

## Key Insights

1. **Doc IR separates concerns**: Formatting rules vs. rendering
2. **Priority groups enable smart expansion**: Last argument first
3. **Fixup is critical**: Moves comments to prevent wrong expansion
4. **Comments don't count toward width**: Prevents formatting instability
5. **Smart indentation**: Multiple nests on one line = single bump
6. **Absorbable patterns**: Keep common patterns compact
7. **Greedy algorithm**: Simple, predictable, fast

## Testing Strategy

1. **Unit tests** for each `Pretty` instance
2. **IR comparison**: Compare `--ir` output with Haskell
3. **Output comparison**: Compare formatted output
4. **Idempotency**: Format twice, must be identical
5. **Fuzzing**: Random Nix code → format → parse → format

## References

- **Haskell source**: `/Users/joerg/git/nixfmt/src/Nixfmt/`
- **Wadler-Leijen paper**: "A prettier printer" (1998)
- **Old Rust attempt**: `/Users/joerg/git/nixfmt-rs-old/src/pretty.rs`
- **Test suite**: `/Users/joerg/git/nixfmt/test/`
