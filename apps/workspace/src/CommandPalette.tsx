import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowRight, ArrowUpRight, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { focus } from "./api";
import { openContextAction } from "./context-actions";
import { queryClient } from "./focus-cache";
import { t } from "./i18n";
import { matchCommands, moveActive, type Command } from "./palette";

/** One way to everything, Ctrl+K: go anywhere, run an action, or find an issue, requirement
 * or change in everything cached. */
export function CommandPalette({
  commands,
  onModel,
}: {
  commands: Command[];
  onModel: (code: string) => void;
}) {
  useTranslation();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLDivElement>(null);
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
  const found = useMemo(() => matchCommands(commands, text), [commands, text]);
  const count = found.length + results.length;
  useEffect(() => setActive(0), [text, query, search.data]);
  useEffect(() => {
    list.current
      ?.querySelector('[data-active="true"]')
      ?.scrollIntoView?.({ block: "nearest" });
  }, [active]);
  function close() {
    setOpen(false);
    setText("");
  }
  function go(code: string) {
    close();
    onModel(code);
  }
  function run(command: Command) {
    close();
    command.run();
  }
  function key(event: React.KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((at) => moveActive(at, step, count));
    } else if (event.key === "Enter" && count) {
      event.preventDefault();
      const command = found[active];
      if (command) run(command);
      else go(results[active - found.length].model);
    }
  }
  return (
    <>
      <button
        type="button"
        className="palette-trigger"
        aria-label={t("search.title")}
        onClick={() => setOpen(true)}
      >
        <Search aria-hidden="true" />
        <span>{t("search.title")}</span>
        <kbd>Ctrl K</kbd>
      </button>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (next) setOpen(true);
          else close();
        }}
      >
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
              onKeyDown={key}
            />
          </div>
          {search.error ? (
            <p className="short" role="alert">
              {t("search.failed")}
            </p>
          ) : null}
          {query && search.isSuccess && !count ? (
            <p className="quiet">{t("search.none")}</p>
          ) : null}
          <div className="palette-list" ref={list}>
            {found.length ? (
              <ul className="palette-commands">
                {found.map((command, at) => (
                  <li key={command.id}>
                    <button
                      type="button"
                      className="palette-command"
                      data-active={at === active}
                      onMouseMove={() => setActive(at)}
                      onClick={() => run(command)}
                    >
                      {command.label}
                      <ArrowRight aria-hidden="true" />
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
            <ul className="search-results">
              {results.map((hit, at) => {
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
                      data-active={at + found.length === active}
                      onMouseMove={() => setActive(at + found.length)}
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
                        <span className="search-hit-snippet">
                          {hit.snippet}
                        </span>
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
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
