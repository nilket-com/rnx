# rnx 0003 evidence: completion

Implementation of `plans/0003_completion.md`, 2026-09-08, on Linux x86_64
against upstream Rune 0.14.1 with no patch. Status of the record stays
proposed until reviewed.

## What landed

- `src/complete.rs`: `complete(buffer, cursor, names)` tokenises the whole
  buffer with Rune's parser (`Parser::parse::<ast::Token>`), finds the
  identifier-or-path group containing the cursor (adjacent `Ident` and `::`
  tokens), takes the text before the cursor as the prefix, and returns the
  whole group's byte range with the candidates. Commands are the one case
  handled outside the lexer: a leading `:` and letters at the start of the
  input. Candidates are deduplicated by replacement text in the order
  bindings, declarations, host paths, commands.
- `src/host.rs`: `install` records each function's path at the point it is
  registered and returns the list; the completer's host names are that
  list, nothing else.
- `src/session.rs`: `binding_names` and `declaration_names` read the
  session's tables.
- `src/repl.rs`: the helper implements the editor's `Completer`, in list
  mode, and overrides `update` so the whole token is replaced and the
  cursor lands after it. The names snapshot is refreshed after every input
  and on `:reset`; a failed input leaves it unchanged because the session
  published nothing.

## Lexical handling, from the parser

End of input is reported by the parser as a zero-width error, not a token;
an unterminated string, template, or block comment is a non-empty error
span from its opening delimiter to the end. A non-empty error span holding
the cursor suppresses completion. A closed string or a comment is not an
identifier token, so a cursor inside one finds no group and gets nothing.
A template is desugared by the parser into synthetic tokens bracketed by an
empty open delimiter at the opening backtick and an empty close at the
closing one, with no close when unfinished; identifiers inside an
interpolation are real tokens, so the template range is tracked from those
delimiters and a cursor inside it, closed or unfinished, gets nothing.
A group preceded by `.` gets nothing, since member completion is not
decided. A group beginning with `::` gets nothing, and command detection
rejects a `::` prefix so no cursor position inside `::alp` reaches the
command list. All byte offsets are the
parser's, so text with multibyte characters before the cursor completes at
the right range.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 ordinary use | pass: `alp` Tab extends to `alpha` and a second Tab lists `alpha` and `alphabet`; `b` Tab completes `alphabet`; `to` completes the declared `total`; `host::wr` completes `host::write_new`; `host::` lists the seven host functions; `:me` completes `:memory`; `std::` changes nothing | `tests/repl.rs::gate_0003_completion_in_the_terminal` |
| 2 mid-token and mid-line | pass: in the terminal, `alpx + 40` with the cursor after `alp` becomes `alpha + 40` and evaluates to 41; in unit tests, `alpha + beta` at offset 3 and `x = alpx + beta` at offset 7 return the whole token's range, and a later line of a multi-line input is found by byte offset | `tests/repl.rs`, `src/complete.rs` tests |
| 3 where not offered | pass: inside `"alp`, `` `alp ``, `/* alp`, after `// alp`, after `obj.alp`, inside a closed string, inside a closed or unfinished template interpolation, after a space, and on an empty buffer, nothing is offered; every cursor position in `::alp` offers nothing; `"héllo" + alp` completes at bytes 11 to 14; `hos` offers `host::`; `::alp` offers nothing | `src/complete.rs` tests |
| 4 after failure and reset | pass: `gone` bound by an input that then panicked is not offered; after `:reset`, `alp` has no candidate while `hos` and `:me` still complete; a name that is both a binding and a declaration is one candidate | `tests/repl.rs`, `src/complete.rs` tests |
| 5 no execution | pass: a binding holding a closure that writes a file is completed and the file does not exist afterwards; `host::write_new` is listed and nothing is written | `tests/repl.rs` |
| 6 one source for host names | pass: the seven registered paths equal the `host::` candidates exactly, and each path compiles as a function value in the context it was registered into | `src/complete.rs::host_source_tests` |
| 7 nothing regresses | pass: the five 0002 terminal gates and the 0002 unit tests are unchanged and green | whole suite |

Test counts: 25 unit tests, 6 pseudo-terminal tests, all green; `cargo fmt
--check` clean; the spike self-checks pass.

## Limits stated

- The standard library is not completed: Rune 0.14.1 has no public
  enumeration of a context's installed items, and no handwritten copy was
  made. `std::` yields nothing, silently.
- A `:` before a word in the middle of an input is lexed as a colon and an
  identifier, so `x :me` offers bindings starting with `me`, which is
  usually none; commands complete at the start of an input only.
- List mode belongs to the editor: the first Tab inserts the common prefix
  and the second Tab lists candidates. The record's "listed" is that second
  Tab.
