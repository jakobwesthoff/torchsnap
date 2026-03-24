import { useCallback, useRef, useState } from "react";
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
          <div className="px-5 py-4">
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search or type a command..."
              className="w-full bg-transparent text-lg text-text-primary placeholder-text-tertiary outline-none"
              // Focus trap: re-focus when blurred so keystrokes always
              // reach the input while the launcher is visible.
              onBlur={() => inputRef.current?.focus()}
            />
          </div>

          {/* Divider */}
          <div className="border-t border-border-divider" />

          {/* Placeholder results area */}
          <div className="px-5 py-8 text-center text-sm text-text-tertiary">
            {query
              ? `Searching for "${query}"…`
              : "Type to search"}
          </div>
        </div>
      </div>
    </div>
  );
}
