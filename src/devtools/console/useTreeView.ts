// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Tree View Hook
//
// Builds a tree from the flat log item stream by grouping
// log messages under their parent spans. Each span becomes
// a collapsible node with its children (messages and nested
// spans) ordered chronologically.
//
// The tree is rebuilt from scratch on each items change via
// useMemo — O(n), fast enough for 10k items.
//
// The tree is then flattened into a list of FlatRows for
// virtualization, respecting the collapse state.
// =========================================================

import { useCallback, useMemo, useState } from "react";
import type { LogItem } from "../types";

// =========================================================
// Tree Data Model
// =========================================================

export interface TreeNode {
  spanId: number;
  /** The span-start item for this node. */
  spanStart: LogItem;
  /** The span-end item, once the span completes. Null while in-progress. */
  spanEnd: LogItem | null;
  /** Direct children in chronological order. */
  children: TreeChild[];
  /** True if the span hasn't ended yet. */
  inProgress: boolean;
}

export type TreeChild =
  | { kind: "item"; item: LogItem }
  | { kind: "span"; node: TreeNode };

// =========================================================
// Flat Rows for Virtualization
// =========================================================

export type FlatRow =
  | { kind: "span-header"; node: TreeNode; depth: number }
  | { kind: "item"; item: LogItem; depth: number };

// =========================================================
// Tree Building
// =========================================================

interface TreeBuildResult {
  rootChildren: TreeChild[];
  spanNodes: Map<number, TreeNode>;
}

function buildTree(items: LogItem[]): TreeBuildResult {
  const spanNodes = new Map<number, TreeNode>();
  const rootChildren: TreeChild[] = [];

  for (const item of items) {
    const { kind } = item;

    if (kind.type === "spanStart") {
      const node: TreeNode = {
        spanId: kind.spanId,
        spanStart: item,
        spanEnd: null,
        children: [],
        inProgress: true,
      };
      spanNodes.set(kind.spanId, node);

      const parentId = kind.parentId;
      if (parentId != null && spanNodes.has(parentId)) {
        spanNodes.get(parentId)!.children.push({ kind: "span", node });
      } else {
        rootChildren.push({ kind: "span", node });
      }
    } else if (kind.type === "spanEnd") {
      const node = spanNodes.get(kind.spanId);
      if (node) {
        node.spanEnd = item;
        node.inProgress = false;
      }
    } else {
      // Regular message — group under its span if one exists.
      if (kind.spanId != null && spanNodes.has(kind.spanId)) {
        spanNodes.get(kind.spanId)!.children.push({ kind: "item", item });
      } else {
        rootChildren.push({ kind: "item", item });
      }
    }
  }

  return { rootChildren, spanNodes };
}

// =========================================================
// Tree Flattening
// =========================================================

function flattenTree(
  children: TreeChild[],
  depth: number,
  collapsed: Set<number>,
  result: FlatRow[],
): void {
  for (const child of children) {
    if (child.kind === "item") {
      result.push({ kind: "item", item: child.item, depth });
    } else {
      const { node } = child;
      result.push({ kind: "span-header", node, depth });
      if (!collapsed.has(node.spanId)) {
        flattenTree(node.children, depth + 1, collapsed, result);
      }
    }
  }
}

// =========================================================
// Hook
// =========================================================

export interface UseTreeViewReturn {
  flatRows: FlatRow[];
  toggleSpan: (spanId: number) => void;
  collapsedSpans: Set<number>;
  resetCollapse: () => void;
}

export function useTreeView(items: LogItem[]): UseTreeViewReturn {
  const [collapsedSpans, setCollapsedSpans] = useState<Set<number>>(
    () => new Set(),
  );

  // Build tree from flat item stream.
  const { rootChildren } = useMemo(() => buildTree(items), [items]);

  // Flatten for virtualization, respecting collapse state.
  const flatRows = useMemo(() => {
    const result: FlatRow[] = [];
    flattenTree(rootChildren, 0, collapsedSpans, result);
    return result;
  }, [rootChildren, collapsedSpans]);

  const toggleSpan = useCallback((spanId: number) => {
    setCollapsedSpans((prev) => {
      const next = new Set(prev);
      if (next.has(spanId)) {
        next.delete(spanId);
      } else {
        next.add(spanId);
      }
      return next;
    });
  }, []);

  const resetCollapse = useCallback(() => {
    setCollapsedSpans(new Set());
  }, []);

  return { flatRows, toggleSpan, collapsedSpans, resetCollapse };
}
