#!/usr/bin/env python3
"""Insert a changelog bullet under `## Unreleased` / a `### section`.

Written after the same mistake happened four times: a global text replace aimed at
a `### section` heading matched a *released* section instead of the unreleased one,
leaving the entry in a version that had already shipped (and sometimes duplicating
the heading). This script finds the `## Unreleased` block first and refuses to
write anywhere else.

Usage:
    python3 scripts/changelog-add.py --section "模型与引擎" --file bullet.md
    python3 scripts/changelog-add.py --section "工具与 CI" -            # stdin
"""

import argparse
import sys

PATH = "CHANGELOG.md"
UNRELEASED = "## Unreleased"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--section", required=True, help="### heading inside Unreleased")
    parser.add_argument("--file", default="-", help="file with the bullet text ('-' = stdin)")
    parser.add_argument("--path", default=PATH)
    args = parser.parse_args()

    if args.file == "-":
        bullet = sys.stdin.read()
    else:
        with open(args.file, encoding="utf-8") as handle:
            bullet = handle.read()
    bullet = bullet.strip("\n")
    if not bullet:
        return fail("the bullet is empty")

    with open(args.path, encoding="utf-8") as handle:
        text = handle.read()

    if not text.lstrip().startswith("# "):
        return fail("CHANGELOG.md should start with a `# ` title")

    # The heading has to be a *line*, not a mention inside prose: an earlier
    # version of this script matched a sentence that quoted `## Unreleased` and
    # happily inserted the entry into a released section.
    def heading_index(source: str) -> int:
        for index, line in enumerate(source.split("\n")):
            if line.rstrip() == UNRELEASED:
                # Convert the line number back to a character offset.
                return sum(len(part) + 1 for part in source.split("\n")[:index])
        return -1

    if heading_index(text) == -1:
        text = text.replace("# Changelog\n", "# Changelog\n\n## Unreleased\n\n", 1)
        if heading_index(text) == -1:
            return fail("could not find or create `## Unreleased`")

    start = heading_index(text)
    # The unreleased block ends at the next version heading.
    nxt = text.find("\n## v", start)
    if nxt == -1:
        return fail("no version heading after `## Unreleased`")
    block = text[start:nxt]

    heading = f"### {args.section}"
    if heading in block:
        # Append under the existing section, right before the next `###`/end.
        section_start = block.index(heading) + len(heading)
        following = block.find("\n### ", section_start)
        insert_at = len(block) if following == -1 else following
        block = block[:insert_at].rstrip("\n") + "\n\n" + bullet + "\n\n" + block[insert_at:]
    else:
        block = block.rstrip("\n") + f"\n\n{heading}\n\n{bullet}\n\n"

    text = text[:start] + block + text[nxt:]
    with open(args.path, "w", encoding="utf-8") as handle:
        handle.write(text)

    print(f"added to `{heading}` under `## Unreleased`")
    return 0


def fail(message: str) -> int:
    print(f"error: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
