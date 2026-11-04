import { t } from "./i18n";

/** Comment markdown as a reader sees it: images become a link, hidden markers go. */
export function readableText(text: string) {
  const clean = text.replace(/<!--[\s\S]*?-->/g, "");
  // Parse inertly, then extract text. Never render source HTML or its styles.
  if (
    !/<\/?(?:span|div|p|br|a|img|pre|code|strong|em|b|i|ul|ol|li|blockquote|h[1-6]|table|tr|td)\b[^>]*>|&(?:#\d+|#x[\da-f]+|[a-z]+);/i.test(
      clean,
    )
  )
    return clean
      .replace(/!\[[^\]]*\]\((\S+?)\)/g, `${t("flow.image")} $1`)
      .trim();
  const doc = new DOMParser().parseFromString(clean, "text/html");
  doc
    .querySelectorAll("script, style, iframe, object")
    .forEach((n) => n.remove());
  doc.querySelectorAll("br").forEach((n) => n.replaceWith("\n"));
  doc.querySelectorAll("a[href]").forEach((n) => {
    const href = n.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) {
      const label = n.textContent || href;
      n.replaceWith(label === href ? href : `[${label}](${href})`);
    }
  });
  doc.querySelectorAll("img").forEach((n) => {
    const src = n.getAttribute("src") ?? "";
    n.replaceWith(
      /^https?:\/\//i.test(src)
        ? `${t("flow.image")} ${src}`
        : (n.getAttribute("alt") ?? ""),
    );
  });
  doc
    .querySelectorAll("p, div, li, pre, blockquote, h1, h2, h3, tr")
    .forEach((n) => n.append("\n"));
  return (doc.body.textContent ?? "")
    .replace(/!\[[^\]]*\]\((\S+?)\)/g, `${t("flow.image")} $1`)
    .trim();
}
