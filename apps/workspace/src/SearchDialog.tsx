import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowUpRight, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { focus } from "./api";
import { openContextAction } from "./context-actions";
import { queryClient } from "./focus-cache";
import { t } from "./i18n";

/** Finds issues, requirements and changes in everything cached; Ctrl+K opens it. */
export function SearchDialog({ onModel }: { onModel: (code: string) => void }) {
  useTranslation();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === "k") {
        event.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  useEffect(() => {
    timer.current = setTimeout(() => setQuery(text.trim()), 200);
    return () => clearTimeout(timer.current);
  }, [text]);
  const search = useQuery(
    {
      queryKey: ["search", query],
      queryFn: async () => (await focus("search", { query })).search,
      enabled: open && query.length > 0 && !query.startsWith("-"),
      staleTime: 30_000,
    },
    queryClient,
  );
  const results = search.data?.results ?? [];
  function go(code: string) {
    setOpen(false);
    onModel(code);
  }
  return (
    <>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={t("search.title")}
        title={`${t("search.title")} (Ctrl+K)`}
        onClick={() => setOpen(true)}
      >
        <Search aria-hidden="true" />
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="search-dialog">
          <DialogTitle className="sr-only">{t("search.title")}</DialogTitle>
          <div className="search-field">
            {search.isFetching ? (
              <Spinner aria-hidden="true" />
            ) : (
              <Search aria-hidden="true" />
            )}
            <Input
              autoFocus
              value={text}
              maxLength={200}
              placeholder={t("search.placeholder")}
              aria-label={t("search.title")}
              onChange={(e) => setText(e.target.value)}
            />
          </div>
          {search.error ? (
            <p className="short" role="alert">
              {t("search.failed")}
            </p>
          ) : null}
          {query && search.isSuccess && !results.length ? (
            <p className="quiet">{t("search.none")}</p>
          ) : null}
          <ul className="search-results">
            {results.map((hit) => {
              const source = openContextAction(
                `open-${hit.id}`,
                t("search.open"),
                hit.url,
              );
              return (
                <li key={`${hit.model}:${hit.type}:${hit.id}`}>
                  <button
                    type="button"
                    className="search-hit"
                    onClick={() => go(hit.model)}
                  >
                    <span className="search-hit-head">
                      <strong>{hit.title || hit.id}</strong>
                      <span className="quiet">{hit.status}</span>
                    </span>
                    <span className="quiet">
                      {[hit.model, hit.id, hit.owner]
                        .filter(Boolean)
                        .join(" · ")}
                    </span>
                    {hit.snippet && hit.snippet !== hit.title ? (
                      <span className="search-hit-snippet">{hit.snippet}</span>
                    ) : null}
                  </button>
                  {source ? (
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      aria-label={source.label}
                      title={source.label}
                      onClick={() => void source.run()}
                    >
                      <ArrowUpRight aria-hidden="true" />
                    </Button>
                  ) : null}
                </li>
              );
            })}
          </ul>
        </DialogContent>
      </Dialog>
    </>
  );
}
