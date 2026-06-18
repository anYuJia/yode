import hljs from "highlight.js/lib/core";
import langBash from "highlight.js/lib/languages/bash";
import langC from "highlight.js/lib/languages/c";
import langCpp from "highlight.js/lib/languages/cpp";
import langCSS from "highlight.js/lib/languages/css";
import langDiff from "highlight.js/lib/languages/diff";
import langGo from "highlight.js/lib/languages/go";
import langHTML from "highlight.js/lib/languages/xml";
import langJava from "highlight.js/lib/languages/java";
import langJavascript from "highlight.js/lib/languages/javascript";
import langJson from "highlight.js/lib/languages/json";
import langMarkdown from "highlight.js/lib/languages/markdown";
import langPython from "highlight.js/lib/languages/python";
import langRust from "highlight.js/lib/languages/rust";
import langSQL from "highlight.js/lib/languages/sql";
import langTOML from "highlight.js/lib/languages/ini";
import langTypescript from "highlight.js/lib/languages/typescript";
import langYaml from "highlight.js/lib/languages/yaml";

type HighlightLanguage = Parameters<typeof hljs.registerLanguage>[1];

const languages: Array<[string, HighlightLanguage]> = [
  ["bash", langBash],
  ["sh", langBash],
  ["shell", langBash],
  ["zsh", langBash],
  ["python", langPython],
  ["py", langPython],
  ["rust", langRust],
  ["rs", langRust],
  ["typescript", langTypescript],
  ["ts", langTypescript],
  ["tsx", langTypescript],
  ["javascript", langJavascript],
  ["js", langJavascript],
  ["jsx", langJavascript],
  ["json", langJson],
  ["toml", langTOML],
  ["ini", langTOML],
  ["yaml", langYaml],
  ["yml", langYaml],
  ["css", langCSS],
  ["html", langHTML],
  ["xml", langHTML],
  ["sql", langSQL],
  ["c", langC],
  ["cpp", langCpp],
  ["go", langGo],
  ["java", langJava],
  ["md", langMarkdown],
  ["markdown", langMarkdown],
  ["diff", langDiff]
];

languages.forEach(([name, language]) => {
  hljs.registerLanguage(name, language);
});
