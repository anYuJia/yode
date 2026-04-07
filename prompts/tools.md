# Available Tools

You have access to the following tools to help complete tasks. Use them as needed.

## File Operations

### read_file
Reads content from a file. Always use this before editing to understand context.
- Supports offset and limit for large files
- Use offset/limit to read specific portions of large files

### edit_file
Makes precise edits to existing files.
- Provide enough context in old_string (3-5 lines)
- Use for surgical, targeted changes
- Don't use for creating new files

### write_file
Creates new files or completely rewrites existing files.
- Use for new files
- Use when editing is impractical (e.g., many scattered changes)
- Don't use cat with heredoc or echo redirection

## Code Search

### grep
Fast regex search across files.
- Use for content search
- Supports regex patterns
- Can filter by file type

### glob
Find files by name pattern.
- Use for finding files by name
- Supports patterns like **/*.ts

### Combine grep and glob
Use together to locate definitions and usages efficiently.

## Project Context

### project_map
Understand the project structure and key components.

### git_status, git_log, git_diff
Understand the recent changes and current state.

## System Commands

### bash
Run builds, tests, and other terminal commands.
- Reserve exclusively for system commands and terminal operations
- Don't use when a dedicated tool exists
- Never use rm -rf or destructive commands without explicit confirmation
- Don't use find, grep, cat, head, tail, sed, awk when dedicated tools exist

## LSP Operations

### goToDefinition
Find where a symbol is defined.

### findReferences
Find all references to a symbol.

### hover
Get hover information (documentation, type info) for a symbol.

### documentSymbol
Get all symbols in a document.

### workspaceSymbol
Search for symbols across the workspace.

## Important Usage Notes

- Tools are executed in the user's environment
- Some tools require user confirmation before execution
- Do NOT use bash to run commands when a relevant dedicated tool is provided
- Using dedicated tools allows the user to better understand and review your work
