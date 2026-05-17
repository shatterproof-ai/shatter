/**
 * Body-driven parameter shape inference (str-yb7q).
 *
 * When the TypeScript checker resolves a parameter to `any`, `unknown`, or
 * an opaque user-defined type, `convertType` returns `{kind: "unknown"}`
 * or `{kind: "opaque"}` and the random input generator emits primitives.
 * A function body that does `data.rows.filter(...)` then throws
 * "data.rows.filter is not a function" on every iteration, burning the
 * exploration budget on shape errors.
 *
 * This pass walks the function body, collects property-access chains
 * rooted at each parameter, and notes which leaves are used like arrays
 * (`.filter`, `.map`, `for-of`, spread in array literal, array
 * destructuring, element access). The result is a synthesised TypeInfo
 * that the input generator realises as a structurally valid object.
 *
 * The pass only fires when the original TypeInfo kind is `unknown` — a
 * checker-derived `array` / `object` / primitive type, or an opaque tag
 * from the static-opacity heuristic, is always preferred. Opaque types
 * carry an intentional "do not synthesize" signal that downstream code
 * uses to skip targets; this pass is not allowed to override it.
 *
 * Extension point: string / number / boolean signals are intentionally
 * NOT inferred here. Array-shape is the high-leverage cluster on the
 * pickpackit workload that motivated this work (str-yb7q); other
 * primitive inferences can be added when a workload justifies them.
 */

import * as ts from "typescript";
import type { ParamInfo, TypeInfo } from "./protocol.js";

/** Same cap as analyzer.ts MAX_TYPE_DEPTH — keeps synthesised trees bounded. */
const MAX_INFER_DEPTH = 32;

/** Array-flavoured method names; any of these at a leaf marks it as an array. */
const ARRAY_METHODS: ReadonlySet<string> = new Set([
  "filter",
  "map",
  "forEach",
  "reduce",
  "reduceRight",
  "find",
  "findIndex",
  "findLast",
  "findLastIndex",
  "some",
  "every",
  "includes",
  "indexOf",
  "lastIndexOf",
  "slice",
  "splice",
  "concat",
  "flat",
  "flatMap",
  "sort",
  "reverse",
  "push",
  "pop",
  "shift",
  "unshift",
  "join",
  "entries",
  "keys",
  "values",
  "at",
  "copyWithin",
  "fill",
  // .length is ambiguous (also strings) but generated arrays still satisfy
  // downstream `.length` reads, so treat it as an array signal.
  "length",
]);

/**
 * Per-parameter property-path trie. Each node tracks whether an array
 * signal was observed at that path; children are keyed by property name.
 */
interface ShapeNode {
  isArray: boolean;
  children: Map<string, ShapeNode>;
}

function newNode(): ShapeNode {
  return { isArray: false, children: new Map() };
}

/**
 * Walk a chain of property/element accesses ending at a parameter
 * identifier and return the parameter name plus the in-order list of
 * property segments. Returns null if the chain doesn't bottom out at
 * a tracked parameter.
 *
 *   `data` → { paramName: "data", segments: [] }
 *   `data.rows` → { paramName: "data", segments: ["rows"] }
 *   `data.rows.filter` → { paramName: "data", segments: ["rows", "filter"] }
 *   `data[0]` → { paramName: "data", segments: [] }  (element access elided)
 *   `data.rows[0]` → { paramName: "data", segments: ["rows"] }
 */
function extractFullChain(
  expr: ts.Expression,
  paramNames: Set<string>,
): { paramName: string; segments: string[] } | null {
  if (ts.isPropertyAccessExpression(expr)) {
    const inner = extractFullChain(expr.expression, paramNames);
    if (!inner) return null;
    return { paramName: inner.paramName, segments: [...inner.segments, expr.name.text] };
  }
  if (ts.isElementAccessExpression(expr)) {
    // Element access dereferences the chain — the receiver itself must
    // be array-shaped, so we don't extend the segment list.
    return extractFullChain(expr.expression, paramNames);
  }
  if (ts.isIdentifier(expr) && paramNames.has(expr.text)) {
    return { paramName: expr.text, segments: [] };
  }
  return null;
}

/**
 * Insert a property path into the trie. Sets `isArray` on the node at the
 * end of the path when `markArrayAtLeaf` is true (path length 0 marks the
 * root node).
 */
function insertPath(
  trie: Map<string, ShapeNode>,
  paramName: string,
  segments: string[],
  markArrayAtLeaf: boolean,
): void {
  const existing = trie.get(paramName);
  let cursor: ShapeNode;
  if (existing) {
    cursor = existing;
  } else {
    cursor = newNode();
    trie.set(paramName, cursor);
  }
  if (segments.length === 0) {
    if (markArrayAtLeaf) cursor.isArray = true;
    return;
  }
  for (let i = 0; i < segments.length; i++) {
    if (i >= MAX_INFER_DEPTH) return;
    const seg = segments[i]!;
    const existingChild = cursor.children.get(seg);
    if (existingChild) {
      cursor = existingChild;
    } else {
      const next: ShapeNode = newNode();
      cursor.children.set(seg, next);
      cursor = next;
    }
  }
  if (markArrayAtLeaf) cursor.isArray = true;
}

/**
 * Walk a function body and build per-parameter shape tries.
 *
 * Exported for unit tests.
 */
export function collectParamShapes(
  body: ts.Node,
  paramNames: Set<string>,
): Map<string, ShapeNode> {
  const trie = new Map<string, ShapeNode>();
  if (paramNames.size === 0) return trie;

  /**
   * Record a usage at `expr`, optionally marking the chain's tail as
   * array-shaped. Returns true if the expression resolved to a tracked
   * parameter chain (used to suppress double-counting when a parent
   * visitor has already handled the chain).
   */
  function recordUsage(expr: ts.Expression, markArray: boolean): boolean {
    const chain = extractFullChain(expr, paramNames);
    if (!chain) return false;
    insertPath(trie, chain.paramName, chain.segments, markArray);
    return true;
  }

  function visit(node: ts.Node): void {
    // Skip nested function bodies — their parameters can shadow ours and
    // their internal usage doesn't constrain our parameters' shapes.
    if (
      ts.isFunctionDeclaration(node) ||
      ts.isFunctionExpression(node) ||
      ts.isArrowFunction(node) ||
      ts.isMethodDeclaration(node)
    ) {
      return;
    }

    // Method call: <chain>.<method>(...)
    // If <method> is an array method, the receiver chain is array-shaped.
    // Either way, register the full path so the leaf appears in the trie.
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression)
    ) {
      const receiver = node.expression.expression;
      const methodName = node.expression.name.text;
      const isArrayCall = ARRAY_METHODS.has(methodName);
      // Mark receiver chain as array if the method is array-flavoured.
      const receiverChain = extractFullChain(receiver, paramNames);
      if (receiverChain) {
        insertPath(trie, receiverChain.paramName, receiverChain.segments, isArrayCall);
      }
    }

    // Property access on its own (or as the receiver of a non-call use):
    // register the full chain so the property appears in the trie. If the
    // tail name is itself an array method, mark the *parent* as array.
    if (ts.isPropertyAccessExpression(node)) {
      const chain = extractFullChain(node, paramNames);
      if (chain) {
        const tail = chain.segments[chain.segments.length - 1];
        if (tail !== undefined && ARRAY_METHODS.has(tail)) {
          // Mark the parent (without the tail) as array.
          insertPath(
            trie,
            chain.paramName,
            chain.segments.slice(0, -1),
            true,
          );
        } else {
          insertPath(trie, chain.paramName, chain.segments, false);
        }
      }
    }

    // Element access: <chain>[i] → the chain is array-shaped.
    if (ts.isElementAccessExpression(node)) {
      recordUsage(node.expression, true);
    }

    // for (const x of <chain>) → chain is array-shaped.
    if (ts.isForOfStatement(node)) {
      // node.expression may itself be a chain.
      recordUsage(node.expression as ts.Expression, true);
    }

    // [...<chain>] inside an array literal → chain is array-shaped.
    if (
      ts.isSpreadElement(node) &&
      node.parent &&
      ts.isArrayLiteralExpression(node.parent)
    ) {
      recordUsage(node.expression, true);
    }

    // const [a, b] = <chain> → chain is array-shaped (array destructuring).
    if (
      ts.isVariableDeclaration(node) &&
      node.initializer &&
      ts.isArrayBindingPattern(node.name)
    ) {
      recordUsage(node.initializer, true);
    }

    ts.forEachChild(node, visit);
  }

  // Visit the body's children rather than the body itself so the body's
  // own SyntaxKind doesn't trip the nested-function guard.
  ts.forEachChild(body, visit);
  return trie;
}

/** Synthesise a TypeInfo from a shape trie node, bounded by depth. */
function nodeToTypeInfo(node: ShapeNode, depth: number): TypeInfo {
  if (depth >= MAX_INFER_DEPTH) return { kind: "unknown" };
  if (node.children.size > 0) {
    const fields: [string, TypeInfo][] = [];
    for (const [name, child] of node.children) {
      fields.push([name, nodeToTypeInfo(child, depth + 1)]);
    }
    if (node.isArray) {
      // Conflicting evidence: the same node was both indexed/iterated
      // AND had named properties accessed. Arrays of objects fit both —
      // emit an array whose element carries the field structure.
      return { kind: "array", element: { kind: "object", fields } };
    }
    return { kind: "object", fields };
  }
  if (node.isArray) {
    return { kind: "array", element: { kind: "unknown" } };
  }
  return { kind: "unknown" };
}

/** True if a synthesised TypeInfo carries no usable structural information. */
function isEmptyShape(t: TypeInfo): boolean {
  if (t.kind === "unknown") return true;
  if (t.kind === "object" && t.fields.length === 0) return true;
  return false;
}

/**
 * Refine parameters whose TypeInfo collapsed to `unknown` or `opaque`,
 * using observed usage in the function body. Returns a new array; does
 * not mutate the input.
 */
export function refineParamShapesFromBody(
  params: ParamInfo[],
  body: ts.Node | undefined,
): ParamInfo[] {
  if (!body || params.length === 0) return params;

  const eligible = new Set(
    params.filter((p) => p.type.kind === "unknown").map((p) => p.name),
  );
  if (eligible.size === 0) return params;

  const trie = collectParamShapes(body, eligible);

  return params.map((p) => {
    if (!eligible.has(p.name)) return p;
    const node = trie.get(p.name);
    if (!node) return p;
    const inferred = nodeToTypeInfo(node, 0);
    if (isEmptyShape(inferred)) return p;
    return { ...p, type: inferred };
  });
}
