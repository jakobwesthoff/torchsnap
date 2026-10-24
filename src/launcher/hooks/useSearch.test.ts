// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// useSearch: which query the view refs belong to
//
// `resultsQuery` names the query whose search set the current
// results, view refs and matched prefix. Until the search of a new
// query answers, all of them still belong to the old one; the
// launcher relies on that to hand a view on screen a query that fits
// its prefix.
// =========================================================

import { act, renderHook, waitFor } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it } from "vitest";
import { mockCommands } from "../../test/tauri";
import type { GadgetViewRef, ResultSource, SearchMessage, SourcedEntry } from "../../types";
import { useSearch } from "./useSearch";

const CALCULATOR_VIEW: GadgetViewRef = { gadgetId: "calculator", view: "history" };
const ECHO_VIEW: GadgetViewRef = { gadgetId: "hello-world", view: "echo" };
const WEATHER_INLINE: GadgetViewRef = { gadgetId: "weather", view: "inline" };

function entry(source: string, id: string): SourcedEntry {
  return {
    id,
    title: id,
    subtitle: null,
    icon: null,
    score: 1,
    titlePositions: [],
    subtitlePositions: [],
    source,
    actions: [{ slot: "primary", label: "Open" }],
  };
}

function results(
  source: ResultSource,
  views: {
    customGadgetView?: GadgetViewRef | null;
    inlineGadgetView?: GadgetViewRef | null;
    matchedPrefix?: string | null;
  } = {},
  entries: SourcedEntry[] = [],
): SearchMessage {
  return {
    type: "searchResults",
    source,
    entries,
    customGadgetView: views.customGadgetView ?? null,
    inlineGadgetView: views.inlineGadgetView ?? null,
    matchedPrefix: views.matchedPrefix ?? null,
  };
}

describe("useSearch resultsQuery", () => {
  // One entry per `search` command, in the order the hook issued them.
  let searches: { query: string; channel: Channel<SearchMessage> }[];

  beforeEach(() => {
    searches = [];
    mockCommands({
      search: ({ query, onResults }) => {
        searches.push({ query, channel: onResults });
      },
    });
  });

  /** The channel of the search for `query`, once the hook issued it.
   *  Each test searches for a query at most once. */
  async function searchFor(query: string): Promise<Channel<SearchMessage>> {
    await waitFor(() => expect(searches.map((s) => s.query)).toContain(query));
    return searches.find((s) => s.query === query)!.channel;
  }

  function answer(channel: Channel<SearchMessage>, message: SearchMessage) {
    act(() => channel.onmessage(message));
  }

  function renderSearch(query: string) {
    return renderHook(({ query }) => useSearch(query), { initialProps: { query } });
  }

  it("starts with an empty resultsQuery", () => {
    const { result } = renderSearch("");
    expect(result.current.resultsQuery).toBe("");
  });

  it("keeps the old query's resultsQuery and prefix until the new search answers", async () => {
    const hook = renderSearch("=2");
    answer(
      await searchFor("=2"),
      results(
        { type: "gadget", id: "calculator" },
        { customGadgetView: CALCULATOR_VIEW, matchedPrefix: "=" },
      ),
    );
    expect(hook.result.current.resultsQuery).toBe("=2");
    expect(hook.result.current.matchedPrefix).toBe("=");

    hook.rerender({ query: "!b" });
    const next = await searchFor("!b");
    expect(hook.result.current.resultsQuery).toBe("=2");
    expect(hook.result.current.matchedPrefix).toBe("=");
    expect(hook.result.current.customGadgetView).toEqual(CALCULATOR_VIEW);

    answer(
      next,
      results(
        { type: "gadget", id: "hello-world" },
        { customGadgetView: ECHO_VIEW, matchedPrefix: "!" },
      ),
    );
    expect(hook.result.current.resultsQuery).toBe("!b");
    expect(hook.result.current.matchedPrefix).toBe("!");
    expect(hook.result.current.customGadgetView).toEqual(ECHO_VIEW);
  });

  it("ignores an answer of a superseded search", async () => {
    const hook = renderSearch("=2");
    const superseded = await searchFor("=2");
    hook.rerender({ query: "!b" });
    await searchFor("!b");

    answer(
      superseded,
      results(
        { type: "gadget", id: "calculator" },
        { customGadgetView: CALCULATOR_VIEW, matchedPrefix: "=" },
      ),
    );

    expect(hook.result.current.resultsQuery).toBe("");
    expect(hook.result.current.matchedPrefix).toBeNull();
    expect(hook.result.current.customGadgetView).toBeNull();
  });

  it("keeps resultsQuery at the query when a later message of its search adds a view", async () => {
    const hook = renderSearch("fire");
    const channel = await searchFor("fire");

    answer(channel, results({ type: "catalog" }, {}, [entry("app-launcher", "firefox")]));
    expect(hook.result.current.resultsQuery).toBe("fire");
    expect(hook.result.current.inlineGadgetView).toBeNull();

    answer(
      channel,
      results({ type: "gadget", id: "weather" }, { inlineGadgetView: WEATHER_INLINE }),
    );
    expect(hook.result.current.resultsQuery).toBe("fire");
    expect(hook.result.current.inlineGadgetView).toEqual(WEATHER_INLINE);
  });

  it("resets resultsQuery with the view refs in the render that empties the query", async () => {
    const hook = renderSearch("=2");
    answer(
      await searchFor("=2"),
      results(
        { type: "gadget", id: "calculator" },
        { customGadgetView: CALCULATOR_VIEW, matchedPrefix: "=" },
      ),
    );

    hook.rerender({ query: "" });

    expect(hook.result.current.resultsQuery).toBe("");
    expect(hook.result.current.matchedPrefix).toBeNull();
    expect(hook.result.current.customGadgetView).toBeNull();
  });
});
