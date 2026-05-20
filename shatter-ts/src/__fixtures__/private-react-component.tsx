/**
 * Kapow-shaped regression fixture for str-jeen.69.
 *
 * Mirrors web/src/components/search/CitationsFlyout.tsx and similar
 * files: a public exported component plus private PascalCase helpers
 * that return JSX. Includes shapes the original fixture did not:
 *
 *   - Imports from external packages (stubbed at runtime by
 *     sandboxRequire).
 *   - A path-alias import (also stubbed).
 *   - A type-only import (erased at transpile).
 *   - An interface declaration (erased at transpile).
 *   - JSX fragments `<>...</>` and nested JSX elements.
 *
 * The private `CitationRow` is the target. Analyzer discovers it via
 * the function-declaration walk; instrumentor must expose it on
 * module.exports so the executor can find it.
 */

import { useState } from "react";
import { Popover, SimpleGrid, Text } from "@mantine/core";
import { semanticColorRoles } from "@/theme";
import type { CardFieldKey } from "@/stores/cardFieldStore";

interface CitationsFlyoutProps {
  visibleCardFields: Set<CardFieldKey>;
  buttonVariant?: "subtle" | "outline";
}

export function CitationsFlyout(props: CitationsFlyoutProps): unknown {
  const [open, setOpen] = useState(false);
  void setOpen;
  return (
    <Popover opened={open} onChange={setOpen}>
      <SimpleGrid cols={2}>
        <Text>{props.buttonVariant ?? "subtle"}</Text>
      </SimpleGrid>
    </Popover>
  );
}

// Private React function component returning JSX with a Fragment and
// branching logic. Modelled exactly after CitationRow in the kapow
// codebase: `function CitationRow({ citation }: { citation: T }) {...}`.
function CitationRow(props: { citation: { label: string; year?: number } }): unknown {
  const sourceSuffix = props.citation.year != null ? ` · ${props.citation.year}` : "";
  return (
    <>
      <Text c={semanticColorRoles.text.emphasis}>{props.citation.label}</Text>
      <Text>{sourceSuffix}</Text>
    </>
  );
}

function _retainPrivateBindings(): unknown {
  return CitationRow;
}
void _retainPrivateBindings;
