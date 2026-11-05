/** Fades the splash from index.html out; a missed transition still removes it. */
export function dismissSplash(doc: Document = document) {
  const splash = doc.getElementById("splash");
  if (!splash) return;
  const remove = () => splash.remove();
  splash.classList.add("done");
  splash.addEventListener("transitionend", remove, { once: true });
  setTimeout(remove, 400);
}
