/**
 * Minimal safe Markdown renderer for AI analysis output.
 *
 * Security model: escape FIRST (`& < > " '`), then only add our own tags —
 * model output can never inject HTML. Supports the subset the analysis prompt
 * actually produces: headings, bullet/numbered lists, fenced code blocks,
 * GFM tables, inline code, bold, italics, blockquotes, hr. Streaming-friendly:
 * an unclosed ``` fence renders the remainder as a code block; an incomplete
 * table renders the rows collected so far.
 */

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Inline spans: `code`, **bold**, *italic*. Input is already escaped. */
function inline(text: string): string {
  return text
    .replace(/`([^`\n]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*\n][^*\n]*?)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, "$1<em>$2</em>");
}

/** GFM table row → cells（容忍首尾缺竖线、单元格两端空白）。 */
function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|")) s = s.slice(0, -1);
  return s.split("|").map((c) => c.trim());
}

/** 分隔行（`| --- | :---: |`）：每个单元格都是 `:---/---/---:/:-:` 形态。 */
function isDelimiterRow(line: string): boolean {
  if (!line.includes("|")) return false;
  const cells = splitRow(line);
  return cells.length > 0 && cells.every((c) => /^:?-+:?$/.test(c));
}

export function renderMarkdown(source: string): string {
  const lines = escapeHtml(source).split("\n");
  const out: string[] = [];

  let paragraph: string[] = [];
  let bullets: string[] = [];
  let ordered = false;
  let inCode = false;
  let codeLines: string[] = [];
  let inQuote = false;
  let quoteLines: string[] = [];
  // GFM 表格：head + body 行（进入条件 = 表头行下一行是分隔行）。
  let table: { head: string[]; body: string[][] } | null = null;

  const flushParagraph = () => {
    if (paragraph.length > 0) {
      out.push(`<p>${paragraph.map(inline).join("<br />")}</p>`);
      paragraph = [];
    }
  };
  const flushList = () => {
    if (bullets.length > 0) {
      const tag = ordered ? "ol" : "ul";
      out.push(`<${tag}>${bullets.map((b) => `<li>${inline(b)}</li>`).join("")}</${tag}>`);
      bullets = [];
    }
  };
  const flushQuote = () => {
    if (quoteLines.length > 0) {
      out.push(`<blockquote>${quoteLines.map(inline).join("<br />")}</blockquote>`);
      quoteLines = [];
    }
  };
  const flushTable = () => {
    if (!table) return;
    const cells = (row: string[], tag: "th" | "td") =>
      `<tr>${row.map((c) => `<${tag}>${inline(c)}</${tag}>`).join("")}</tr>`;
    out.push(
      `<table><thead>${cells(table.head, "th")}</thead><tbody>${table.body
        .map((r) => cells(r, "td"))
        .join("")}</tbody></table>`
    );
    table = null;
  };
  const flushAll = () => {
    flushParagraph();
    flushList();
    flushQuote();
    flushTable();
  };

  for (let idx = 0; idx < lines.length; idx++) {
    const line = lines[idx];
    // Fenced code blocks take precedence over everything else.
    if (inCode) {
      if (/^\s*```/.test(line)) {
        out.push(`<pre><code>${codeLines.join("\n")}</code></pre>`);
        codeLines = [];
        inCode = false;
      } else {
        codeLines.push(line);
      }
      continue;
    }
    if (/^\s*```/.test(line)) {
      flushAll();
      inCode = true;
      continue;
    }

    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      flushAll();
      const level = Math.min(heading[1].length + 2, 6); // # → h3 … #### → h6
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`);
      continue;
    }

    if (/^\s*(---+|\*\*\*+)\s*$/.test(line)) {
      flushAll();
      out.push("<hr />");
      continue;
    }

    // GFM 表格：表头行（以 | 开头）且下一行是分隔行才成表；
    // 流式时表头先到、分隔行还没到 → 暂按普通段落，下个 delta 会重渲。
    if (/^\s*\|/.test(line)) {
      if (table === null && idx + 1 < lines.length && isDelimiterRow(lines[idx + 1])) {
        flushAll();
        table = { head: splitRow(line), body: [] };
        idx++; // 跳过分隔行
        continue;
      }
      if (table !== null) {
        table.body.push(splitRow(line));
        continue;
      }
    } else if (table !== null) {
      flushTable();
    }

    const quote = /^\s*&gt;\s?(.*)$/.exec(line);
    if (quote) {
      flushParagraph();
      flushList();
      flushTable();
      if (!inQuote) {
        inQuote = true;
      }
      quoteLines.push(quote[1]);
      continue;
    }
    if (inQuote) {
      flushQuote();
      inQuote = false;
    }

    const bullet = /^\s*[-*•]\s+(.*)$/.exec(line);
    if (bullet) {
      flushParagraph();
      flushQuote();
      flushTable();
      if (bullets.length === 0) {
        ordered = false;
      } else if (ordered) {
        flushList(); // ordered → unordered switch
      }
      bullets.push(bullet[1]);
      continue;
    }

    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    if (numbered) {
      flushParagraph();
      flushQuote();
      flushTable();
      if (bullets.length === 0) {
        ordered = true;
      } else if (!ordered) {
        flushList(); // unordered → ordered switch
      }
      bullets.push(numbered[1]);
      continue;
    }

    flushList();
    flushTable();
    if (line.trim() === "") {
      flushParagraph();
    } else {
      paragraph.push(line.trim());
    }
  }

  // Streaming tail: unclosed fence renders as code so far.
  if (inCode) out.push(`<pre><code>${codeLines.join("\n")}</code></pre>`);
  flushAll();
  return out.join("");
}
