#!/bin/bash

# Check the Rust sources present in the Git worktree, including new files. Build
# output and ignored local worktrees stay outside this list.
copyright_pattern='^// Copyright (19|20)[0-9]{2} (.+ and )?the (Runebender( Xilem)?|Xilem) Authors( and .+)?$'
license_pattern='^// SPDX-License-Identifier: Apache-2\.0( OR MIT)?$'
missing=()

while IFS= read -r file; do
    [[ -f $file ]] || continue
    copyright_line=$(sed -n '1p' "$file")
    license_line=$(sed -n '2p' "$file")
    if [[ ! $copyright_line =~ $copyright_pattern || ! $license_line =~ $license_pattern ]]; then
        missing+=("$file")
    fi
done < <(git ls-files --cached --others --exclude-standard '*.rs')

if ((${#missing[@]})); then
    printf 'The following Rust files lack the standard copyright header:\n\n'
    printf '  %s\n' "${missing[@]}"
    printf '\nAdd a Runebender copyright line and SPDX license identifier.\n'
    exit 1
fi

echo 'All tracked Rust files have copyright headers.'
