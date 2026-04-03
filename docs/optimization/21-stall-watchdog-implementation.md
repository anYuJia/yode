# Stall Watchdog Implementation Deep Dive

## Overview

Claude Code implements a stall watchdog mechanism to detect when background bash commands become blocked on interactive input prompts. This prevents silent hangs where commands wait indefinitely for user input that never arrives.

**File**: `src/tasks/LocalShellTask/LocalShellTask.tsx`

---

## Constants

```typescript
const STALL_CHECK_INTERVAL_MS = 5_000      // Check every 5 seconds
const STALL_THRESHOLD_MS = 45_000          // Stall detected after 45 seconds
const STALL_TAIL_BYTES = 1024              // Read last 1KB for prompt detection
```

### Rationale

| Constant | Value | Rationale |
|----------|-------|-----------|
| **STALL_CHECK_INTERVAL_MS** | 5 seconds | Balances responsiveness with resource usage; frequent enough to catch stalls quickly, infrequent enough to avoid I/O thrashing |
| **STALL_THRESHOLD_MS** | 45 seconds | Long enough to not trigger for legitimately slow commands (git log -S, large builds), short enough to catch real interactive blocks before user notices |
| **STALL_TAIL_BYTES** | 1024 bytes | Minimizes I/O while capturing enough context to detect prompt patterns |

---

## Prompt Pattern Detection

```typescript
const PROMPT_PATTERNS = [
  /\(y\/n\)/i,                                          // (Y/n), (y/N)
  /\[y\/n\]/i,                                          // [Y/n], [Y/N]
  /\(yes\/no\)/i,                                       // (yes/no)
  /\b(?:Do you|Would you|Shall I|Are you sure|Ready to)\b.*\? *$/i,  // Directed questions
  /Press (any key|Enter)/i,                             // Press any key / Press Enter
  /Continue\?/i,                                        // Continue?
  /Overwrite\?/i                                        // Overwrite?
]

function looksLikePrompt(tail: string): boolean {
  const lastLine = tail.trimEnd().split('\n').pop() ?? ''
  return PROMPT_PATTERNS.some(p => p.test(lastLine))
}
```

### Pattern Categories

| Category | Examples | Why Detected |
|----------|----------|--------------|
| **Confirmation prompts** | `(y/n)`, `[Y/n]`, `(yes/no)` | Binary choice prompts |
| **Directed questions** | `Do you want to...?`, `Would you like to...?` | Questions directed at user |
| **Action prompts** | `Press Enter`, `Press any key` | Waiting for key press |
| **Confirmation** | `Continue?`, `Overwrite?` | Single-word confirmations |

### Last-Line Matching Logic

```typescript
const lastLine = tail.trimEnd().split('\n').pop() ?? ''
```

**Why last line only**: Prompts appear on the **last line** of output. Earlier content may be command output, logs, or error messages that are irrelevant to the stall detection.

**Example**:
```
$ git push
Username for 'https://github.com': 
                         ^^^^^^^^ ← This line matches pattern
```

---

## Stall Watchdog Implementation

```typescript
function startStallWatchdog(
  taskId: string,
  description: string,
  kind: BashTaskKind | undefined,
  toolUseId?: string,
  agentId?: AgentId
): () => void {
  // Monitor mode: no watchdog (expected to run indefinitely)
  if (kind === 'monitor') return () => {}
  
  const outputPath = getTaskOutputPath(taskId)
  let lastSize = 0
  let lastGrowth = Date.now()
  let cancelled = false
  
  const timer = setInterval(() => {
    void stat(outputPath).then(s => {
      // File grew - reset stall timer
      if (s.size > lastSize) {
        lastSize = s.size
        lastGrowth = Date.now()
        return
      }
      
      // Not stalled long enough yet
      if (Date.now() - lastGrowth < STALL_THRESHOLD_MS) return
      
      // Check if output ends with a prompt
      void tailFile(outputPath, STALL_TAIL_BYTES).then(({ content }) => {
        if (cancelled) return
        if (!looksLikePrompt(content)) {
          // Not a prompt - keep watching, reset timer
          lastGrowth = Date.now()
          return
        }
        
        // Prompt detected - fire notification
        cancelled = true
        clearInterval(timer)
        
        const toolUseIdLine = toolUseId ? `\n<${TOOL_USE_ID_TAG}>${toolUseId}</${TOOL_USE_ID_TAG}>` : ''
        const summary = `${BACKGROUND_BASH_SUMMARY_PREFIX}"${description}" appears to be waiting for interactive input`
        
        const message = `<${TASK_NOTIFICATION_TAG}>
<${TASK_ID_TAG}>${taskId}</${TASK_ID_TAG}>${toolUseIdLine}
<${OUTPUT_FILE_TAG}>${outputPath}</${OUTPUT_FILE_TAG}>
<${SUMMARY_TAG}>${escapeXml(summary)}</${SUMMARY_TAG}>
</${TASK_NOTIFICATION_TAG}>
Last output:
${content.trimEnd()}

The command is likely blocked on an interactive prompt. Kill this task and re-run with piped input (e.g., \`echo y | command\`) or a non-interactive flag if one exists.`
        
        enqueuePendingNotification({
          value: message,
          mode: 'task-notification',
          priority: 'next',
          agentId
        })
      }, () => {})
    }, () => {})  // File may not exist yet
  }, STALL_CHECK_INTERVAL_MS)
  
  timer.unref()  // Don't prevent process exit
  
  // Return cancel function
  return () => {
    cancelled = true
    clearInterval(timer)
  }
}
```

---

## State Machine

```
┌─────────────────────────────────────────────────────────────┐
│                      START WATCHDOG                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Every 5 seconds: check output file size                     │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
              ▼                               ▼
     ┌─────────────────┐             ┌─────────────────┐
     │ File grew?      │             │ No growth       │
     │ (s.size > last) │             │                 │
     └─────────────────┘             └─────────────────┘
              │                               │
              │ YES                           │ Check duration
              ▼                               │
     ┌─────────────────┐                      │
     │ Reset:          │                      │
     │ lastSize = s.size                      │
     │ lastGrowth = now                       │
     │ Return (keep watching)                 │
     └─────────────────┘                      │
                                              ▼
                                   ┌─────────────────────┐
                                   │ Stalled > 45s?      │
                                   │ (now - lastGrowth   │
                                   │  < STALL_THRESHOLD) │
                                   └─────────────────────┘
                                              │
                                ┌─────────────┴─────────────┐
                                │ NO                        │ YES
                                ▼                           ▼
                   ┌─────────────────────┐      ┌─────────────────────┐
                   │ Reset timer         │      │ tailFile(last 1KB)  │
                   │ lastGrowth = now    │      └─────────────────────┘
                   │ Keep watching       │                 │
                   └─────────────────────┘                 │
                                                           ▼
                                                ┌─────────────────────┐
                                                │ looksLikePrompt()?  │
                                                └─────────────────────┘
                                                           │
                                           ┌───────────────┴───────────────┐
                                           │ NO                            │ YES
                                           ▼                               ▼
                                  ┌─────────────────────┐      ┌─────────────────────┐
                                  │ Reset timer         │      │ Cancel timer        │
                                  │ lastGrowth = now    │      │ Send notification   │
                                  │ Keep watching       │      │ (priority: next)    │
                                  └─────────────────────┘      └─────────────────────┘
```

---

## Notification Message Structure

```xml
<task_notification>
  <task_id>task_12345</task_id>
  <tool_use_id>toolu_abc123</tool_use_id>  <!-- optional -->
  <output_file>/path/to/output</output_file>
  <summary>Background command "git push" appears to be waiting for interactive input</summary>
</task_notification>
Last output:
Username for 'https://github.com': 

The command is likely blocked on an interactive prompt. Kill this task and re-run with piped input (e.g., `echo y | command`) or a non-interactive flag if one exists.
```

### XML Tag Reference

| Tag | Purpose |
|-----|---------|
| `<task_id>` | Unique task identifier for cancellation |
| `<tool_use_id>` | Original tool use ID (optional, for traceability) |
| `<output_file>` | Path to task output file for inspection |
| `<summary>` | Human-readable description (prefixed with `BACKGROUND_BASH_SUMMARY_PREFIX`) |

### Why No `<status>` Tag?

The notification deliberately omits a `<status>` tag:

```typescript
// No <status> tag — print.ts treats <status> as a terminal
// signal and an unknown value falls through to 'completed',
// falsely closing the task for SDK consumers.
```

**SDK behavior**: Unknown status values fall through to `'completed'`, which would falsely mark the task as done for SDK consumers (VS Code extension, etc.).

---

## Integration Points

### Where Watchdog is Started

```typescript
// In createBackgroundTask():
const cancelStallWatchdog = startStallWatchdog(taskId, description, kind, toolUseId, agentId)

// Result handler cancels watchdog on completion:
void shellCommand.result.then(async result => {
  cancelStallWatchdog()  // Stop watching
  await flushAndCleanup(shellCommand)
  // ... rest of cleanup
})
```

### Monitor Mode Exemption

```typescript
if (kind === 'monitor') return () => {}
```

**Rationale**: Monitor mode commands (e.g., `tail -f logs`, `top`, `htop`) are expected to run indefinitely and produce continuous output. They should not trigger stall detection.

### Grace Period on Exit

```typescript
timer.unref()  // Don't prevent process exit
```

**Rationale**: The watchdog timer should not prevent the Claude Code process from exiting if all other work is complete.

---

## Edge Cases Handled

| Edge Case | Handling |
|-----------|----------|
| **File doesn't exist yet** | `stat()` error silently ignored (`() => {}`) |
| **Command completes normally** | `cancelStallWatchdog()` called in result handler |
| **User kills task** | `cancelled = true` prevents notification race |
| **Monitor mode** | Returns no-op function immediately |
| **Slow command (not stalled)** | Any file growth resets `lastGrowth` |
| **Stalled but not a prompt** | `lastGrowth` reset, watchdog continues |
| **Multiple ticks during notification** | `cancelled = true` latched before async boundary |

---

## Key Design Decisions

### 1. Fail-Safe Alerting (Not Fail-Closed)

Unlike the security layers (AST analysis, permission matching) which are **fail-closed** (uncertainty → block), the stall watchdog is **fail-safe** (uncertainty → alert).

**Rationale**: A false positive (alerting on a slow command) is harmless - the user can dismiss it. A false negative (missing a real stall) leaves the command hanging silently.

### 2. Growth Resets Everything

Any file growth (even 1 byte) resets the stall timer completely.

**Rationale**: 
- Simple state machine (no need to track growth rate)
- Slow commands that produce occasional output won't trigger false alarms
- The 45-second threshold provides sufficient debounce

### 3. Prompt Pattern Matching on Last Line Only

Only the last line of output is checked against prompt patterns.

**Rationale**:
- Prompts always appear on the last line
- Earlier content is irrelevant (command output, logs, errors)
- Minimizes false positives from prompts mentioned in output text

### 4. One-Shot Notification

The watchdog fires exactly once per task, then cancels itself.

**Rationale**:
- Repeated notifications would be annoying
- The model/ user has been informed; further action is their choice
- Prevents notification spam for long-running stalled tasks

---

## Security Considerations

### Not a Security Feature

The stall watchdog is **not** a security mechanism. It's a **usability feature** that:

- Helps the model identify blocked commands
- Provides actionable guidance (use piped input or non-interactive flags)
- Prevents silent hangs that waste user time

### Does Not Replace Permission Checks

Commands that pass permission checks but prompt for input are still executed - the watchdog only alerts, it doesn't block.

**Example**: `git push` to an HTTPS remote may prompt for credentials. This passes permission checks (git is safe) but stalls waiting for input.

---

## Summary

| Aspect | Implementation |
|--------|----------------|
| **Check interval** | 5 seconds |
| **Stall threshold** | 45 seconds |
| **Tail size** | 1024 bytes |
| **Prompt patterns** | 7 regex patterns (confirmation, questions, key press) |
| **Notification** | One-shot, priority: 'next', no `<status>` tag |
| **Exemptions** | Monitor mode tasks |
| **Cancellation** | Explicit cancel function, latched `cancelled` flag |
