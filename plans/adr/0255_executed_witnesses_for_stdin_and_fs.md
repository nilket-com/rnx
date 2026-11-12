# ADR 0255: executed witnesses for standard input and the file system

Status: Proposed

Base: published 5ed9692bd28016c07b0af4057b9ec2ec7c9dbc9c (ADR 0254).

## Problem

Sixteen host functions are in the class "Admitted, not executed". The
execution engine accepts a call to each of them, and nothing has ever run
one in both engines and compared what happened. Thirteen of the sixteen
act on the outside world: one reads standard input and twelve act on the
file system.

	io::stdin
	fs::read, fs::exists, fs::absolute, fs::mkdir, fs::mkdir_all,
	fs::remove_file, fs::remove_dir, fs::remove_dir_all, fs::cwd,
	fs::temp_dir, fs::copy, fs::rename

They were left unexecuted because a comparison must not read the
developer's terminal or touch the developer's files. A placeholder call
with an empty path would run, and would show only that both engines refuse
an empty path. Admission is a statement about the compiler. It says
nothing about whether the call does the same thing.

## Goal

Each of the thirteen is run in both engines under the same private
conditions, and its row carries the class the comparison shows.

## Required behavior

- Scope is the thirteen rows. The three clock rows stay "Admitted, not
  executed"; two samples of a clock are never equal, and their observation
  rule is its own record.
- Each engine runs in a child process with its own private directory, its
  own working directory inside it, and a controlled environment and
  standard input. The two private locations have paths of equal length so
  that any path in a result can be compared after the private root is
  replaced by one fixed marker. Nothing outside the private locations is
  read for content or written.
- A witness compares the returned value or error and the resulting state
  of the private directory: which entries exist, their kinds and their
  contents. A function with an effect has at least one successful call
  and at least one refused or failing call. An effect happens once.
- Standard input is fed known bytes and then end of input. The witness
  covers a read with data and a read at end of input. The developer's
  terminal is never read; a control shows the child's input is the
  supplied one.
- fs::cwd, fs::temp_dir and fs::absolute return paths that depend on the
  machine. They are compared under the marker rule above and against the
  controlled working directory and environment of the child, not against
  each other's raw text. The rule is stated once and applies only to the
  private root.
- A row is classed only by what its comparison shows, under the existing
  classes and rules. No row becomes Equal by declaration, and no new class
  or wording rule is introduced. A difference stays visible as a row with
  its two exact outcomes. A wrong result in the execution engine is a
  stop: send it; the fix is its own record.
- The pinned table contains no machine path, user name, process number or
  time. Captured environments are not committed.
- Private directories are removed after a pass. After a failure they may
  remain for diagnosis, and the failure message names them.

## Out of scope

- Engine, session and host sources. This record changes tests, the parity
  generator and the pinned table.
- Clocks, the receiver rows, the functions still refused.
- Manifest and lock files.

## Acceptance

- session_parity.json: "Admitted, not executed" falls from 16 to 3. The
  record states the class of each of the thirteen rows and the totals
  before and after. No other row changes class.
- Controls: a witness that would write outside its private directory
  fails before the call; an effect performed twice fails; a difference in
  resulting directory state with equal return values is classed as a
  different result; the marker replaces only the private root; standard
  input comes only from the supplied bytes.
- The generated table is identical on a second run and from a different
  working directory.
- Required stages follow the changed paths.
