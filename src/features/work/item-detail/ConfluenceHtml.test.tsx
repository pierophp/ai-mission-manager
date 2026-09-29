// @vitest-environment happy-dom
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ConfluenceHtml } from "./ConfluenceHtml";

describe("ConfluenceHtml", () => {
  it("renders allowed page formatting and strips active content and unsafe links", () => {
    const html = renderToStaticMarkup(
      createElement(ConfluenceHtml, {
        html: '<p>Safe <strong>body</strong></p><script>window.compromised = true</script><a href="javascript:alert(1)">unsafe link</a>',
      }),
    );

    expect(html).toContain("<p>Safe <strong>body</strong></p>");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("javascript:");
    expect(html).toContain(">unsafe link</a>");
  });
});
