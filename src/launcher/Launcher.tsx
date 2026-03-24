import { useCallback, useRef, useState } from "react";
import { MagnifyingGlassIcon } from "@heroicons/react/24/outline";
import { useWindowLifecycle } from "./hooks/useWindowLifecycle";

export function Launcher() {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const mouseActiveRef = useRef(false);

  const { hide } = useWindowLifecycle({
    inputRef,
    mouseActiveRef,
    resetState: useCallback(() => {
      setQuery("");
    }, []),
  });

  return (
    <div
      className="fixed inset-0 flex flex-col items-center pt-[25vh]"
      onClick={hide}
    >
      <div className="relative" onClick={(e) => e.stopPropagation()}>
        {/* Launcher card */}
        <div
          className="w-[680px] rounded-2xl bg-surface overflow-hidden"
          style={{
            boxShadow: [
              "0 0 0 1px var(--color-border)",
              "inset 0 1px 0 0 rgba(255,255,255,0.06)",
              "0 4px 16px rgba(0,0,0,0.12)",
              "0 16px 48px rgba(0,0,0,0.16)",
            ].join(", "),
          }}
        >
          {/* Search input */}
          <div className="flex items-center gap-3 px-5 py-4">
            <MagnifyingGlassIcon className="h-5 w-5 shrink-0 text-accent" />
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Type to search"
              className="flex-1 text-lg bg-transparent focus:outline-none placeholder:text-text-muted text-text-primary"
              // Focus trap: re-focus when blurred so keystrokes always
              // reach the input while the launcher is visible.
              onBlur={() => inputRef.current?.focus()}
            />
            <kbd className="shrink-0 rounded border border-border bg-surface-inset px-1.5 py-0.5 font-mono text-[11px] text-text-muted shadow-[0_1px_0_var(--color-border)]">
              ESC
            </kbd>
          </div>
        </div>
      </div>
    </div>
  );
}
