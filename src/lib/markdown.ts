/**
 * Minimal safe Markdown renderer for AI analysis output.
 *
 * Security model: escape FIRST (`& < > " '`), then only add our own tags —
 * model output can never inject HTML. Supports the subset the analysis prompt
 * actually produces: headings, bullet/numbered lists, fenced code blocks,
 * inline code, bold, italics, blockquotes, hr. Streaming-friendly: an
 * unclosed ``` fence renders the remainder as a code block.
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
  const flushAll = () => {
    flushParagraph();
    flushList();
    flushQuote();
  };

  for (const line of lines) {
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

    const quote = /^\s*&gt;\s?(.*)$/.exec(line);
    if (quote) {
      flushParagraph();
      flushList();
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
      if (bullets.length === 0) {
        ordered = true;
      } else if (!ordered) {
        flushList(); // unordered → ordered switch
      }
      bullets.push(numbered[1]);
      continue;
    }

    flushList();
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
