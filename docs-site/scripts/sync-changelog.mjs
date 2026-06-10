// Copies the repo-root CHANGELOG.md (owned by the release pipeline) into the
// docs tree with Docusaurus frontmatter. Runs via the prestart/prebuild hooks;
// the generated docs/changelog.md is gitignored.
import { readFileSync, writeFileSync } from 'node:fs'

const src = new URL('../../CHANGELOG.md', import.meta.url)
const dest = new URL('../docs/changelog.md', import.meta.url)

const frontmatter = `---
slug: /changelog
sidebar_label: Changelog
description: Release notes for Ante
---

`

writeFileSync(dest, frontmatter + readFileSync(src, 'utf8'))
console.log('synced CHANGELOG.md -> docs/changelog.md')
