/**
 * Security + behavior tests for the hand-rolled markdown renderer (B5).
 * The escape-first model is the whole point of not using a library — these
 * tests pin it down.
 */
import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown — escape-first security model", () => {
  it("escapes raw HTML in plain text", () => {
    expect(renderMarkdown("<img src=x onerror=alert(1)>")).toBe(
      "<p>&lt;img src=x onerror=alert(1)&gt;</p>"
    );
  });

  it("escapes script tags inside inline formatting", () => {
    const out = renderMarkdown("**<script>alert(1)</script>**");
    expect(out).toContain("&lt;script&gt;");
    expect(out).not.toContain("<script>");
  });

  it("escapes HTML inside code fences", () => {
    const out = renderMarkdown("```\n<script>alert(1)</script>\n```");
    expect(out).toBe("<pre><code>&lt;script&gt;alert(1)&lt;/script&gt;</code></pre>");
  });

  it("escapes quotes and apostrophes", () => {
    const out = renderMarkdown('say "hi" and it\'s');
    expect(out).toContain("&quot;hi&quot;");
    expect(out).toContain("it&#39;s");
  });

  it("escapes HTML in table cells", () => {
    const out = renderMarkdown("| a | b |\n| --- | --- |\n| <b> | x |");
    expect(out).toContain("&lt;b&gt;");
    expect(out).not.toContain("<b>");
  });

  it("allows only the renderer's own tags (no raw pass-through)", () => {
    const out = renderMarkdown("[link](x) <div>y</div>");
    expect(out).not.toContain("<div>");
    expect(out).not.toContain("<a ");
  });
});

describe("renderMarkdown — structure", () => {
  it("renders headings (# → h3 … #### → h6)", () => {
    expect(renderMarkdown("# Title")).toBe("<h3>Title</h3>");
    expect(renderMarkdown("#### Deep")).toBe("<h6>Deep</h6>");
  });

  it("renders bullet and ordered lists", () => {
    expect(renderMarkdown("- a\n- b")).toBe("<ul><li>a</li><li>b</li></ul>");
    expect(renderMarkdown("1. a\n2. b")).toBe("<ol><li>a</li><li>b</li></ol>");
  });

  it("renders inline code, bold, italics", () => {
    expect(renderMarkdown("`x` **b** *i*")).toBe("<p><code>x</code> <strong>b</strong> <em>i</em></p>");
  });

  it("renders blockquotes and hr", () => {
    expect(renderMarkdown("> quoted")).toBe("<blockquote>quoted</blockquote>");
    expect(renderMarkdown("---")).toBe("<hr />");
  });

  it("renders GFM tables", () => {
    const out = renderMarkdown("| h1 | h2 |\n| --- | --- |\n| a | b |");
    expect(out).toBe(
      "<table><thead><tr><th>h1</th><th>h2</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr></tbody></table>"
    );
  });
});

describe("renderMarkdown — streaming friendliness", () => {
  it("renders an unclosed fence as the code collected so far", () => {
    expect(renderMarkdown("```\nline1\nline2")).toBe("<pre><code>line1\nline2</code></pre>");
  });

  it("renders an incomplete table as collected rows (head only)", () => {
    const out = renderMarkdown("| h1 | h2 |\n| --- | --- |\n| a | b");
    expect(out).toContain("<th>h1</th>");
    expect(out).toContain("<td>a</td>");
  });

  it("a bare table header without a delimiter row stays a paragraph", () => {
    expect(renderMarkdown("| h1 | h2 |")).toBe("<p>| h1 | h2 |</p>");
  });
});
