import { Fragment, createElement, useMemo } from "react";

const allowedTags = new Set([
  "a", "blockquote", "br", "code", "dd", "div", "dl", "dt", "em", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "li", "ol", "p", "pre", "s", "span", "strong", "sub", "sup", "table", "tbody", "td", "th", "thead", "tr", "u", "ul",
]);

function safeHref(value: string | null): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value, window.location.origin);
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? value : undefined;
  } catch {
    return undefined;
  }
}

/** Render read-only Confluence HTML through a small element and attribute allowlist. */
export function ConfluenceHtml({ html }: { html: string }) {
  const content = useMemo(() => {
    if (typeof DOMParser === "undefined") return html;
    const document = new DOMParser().parseFromString(html, "text/html");
    const renderNode = (node: Node, key: string): React.ReactNode => {
      if (node.nodeType === Node.TEXT_NODE) return node.textContent;
      if (!(node instanceof Element) || !allowedTags.has(node.tagName.toLowerCase())) return null;
      const tag = node.tagName.toLowerCase();
      const props: Record<string, string> = {};
      if (tag === "a") {
        const href = safeHref(node.getAttribute("href"));
        if (href) {
          props.href = href;
          props.rel = "noreferrer noopener";
        }
      }
      return createElement(tag, { ...props, key }, Array.from(node.childNodes).map((child, index) => renderNode(child, `${key}.${index}`)));
    };
    return Array.from(document.body.childNodes).map((node, index) => renderNode(node, String(index)));
  }, [html]);
  return <div className="spec-markdown">{content ?? <Fragment />}</div>;
}
