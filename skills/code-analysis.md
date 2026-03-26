---
name: code-analysis
description: Structured code analysis workflow — use when asked to analyze, review, or audit a codebase
---

# Code Analysis Skill

When asked to analyze, review, or audit code, follow this structured workflow.

## Step 1: Build Mental Model

Call `project_map` first to get the project overview.

```
project_map(depth: 2, include_deps: true)
```

Review the output and note:
- Project type and scale
- Key entry points
- Module boundaries
- Critical dependencies

## Step 2: Identify Risk Areas

Based on the project map, identify high-risk areas to investigate:
- Modules with many dependencies (high coupling)
- Entry points that handle external input (security surface)
- Data access layers (correctness, injection risks)
- Error handling boundaries (resilience)
- Authentication/authorization paths (security)
- Concurrency patterns (race conditions, deadlocks)

## Step 3: Form Hypotheses

For each risk area, create a hypothesis using the `hypothesis` tool:

```
hypothesis(action: "create", hypothesis: "...", evidence_needed: "...", type: "BUG|RISK|OPTIMIZATION")
```

Rules:
- Be specific: "SQL injection in user search endpoint" NOT "security issues"
- State what evidence would confirm OR refute
- Classify correctly: BUG (definite defect), RISK (potential issue), OPTIMIZATION (improvement)
- Do NOT create hypotheses for design choices unless they violate stated requirements

## Step 4: Verify Each Hypothesis

For each pending hypothesis:
1. Read the relevant code (use `read_file`, `grep`)
2. Trace the call chain across files
3. Check BOTH confirming and disconfirming evidence
4. A single file match is NOT enough — cross-reference

Then mark each hypothesis:
```
hypothesis(action: "verify", id: "h1", evidence: "file.rs:45 ...", confidence: "HIGH|MEDIUM|LOW")
```
or
```
hypothesis(action: "refute", id: "h1", evidence: "Actually uses prepared statements at db.rs:34")
```

## Step 5: Generate Report

```
hypothesis(action: "report")
```

This generates a structured report with:
- Verified findings grouped by type (BUG > RISK > OPTIMIZATION)
- Confidence and evidence for each finding
- Refuted hypotheses (showing what was ruled out)
- Pending hypotheses (if any remain unverified)

## Step 6: Save to Memory

If the analysis reveals important architectural insights, save them to memory for future reference:
```
memory(action: "write", key: "analysis/<project>", content: "...")
```

## Guidelines

- **Budget**: Stay within 25 tool calls. At 15, start summarizing.
- **Confidence calibration**:
  - HIGH = multi-file evidence, reproduced the issue path
  - MEDIUM = single file evidence + sound reasoning
  - LOW = observation only, no cross-reference
- **Do NOT**:
  - Report LOW confidence findings as critical
  - Mix optimization suggestions with actual bugs
  - Claim performance issues without data
  - Report design choices as bugs
- **Do**:
  - Start with entry points, not random files
  - Follow imports to understand data flow
  - Check error handling paths, not just happy paths
  - Note what you DIDN'T find (absence of evidence matters)
