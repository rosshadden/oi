# generate the examples page from ./examples/
[group("docs")]
examples:
	#!/usr/bin/env bash
	{
		printf '+++\ntitle = "examples"\n+++\n'
		for f in examples/*.oi; do
			printf '\n## %s\n\n```oi\n' "$(basename "$f" .oi)"
			cat "$f"
			printf '```\n'
		done
	} > www/content/examples.md
