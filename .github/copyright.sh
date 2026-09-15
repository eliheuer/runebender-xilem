#!/bin/bash

# Check the Rust sources tracked by Git. Build output and local worktrees are
# deliberately outside this list, so the result matches a clean CI checkout.
copyright_pattern='^// Copyright (19|20)[0-9]{2} (.+ and )?the (Runebender( Xilem)?|Xilem) Authors( and .+)?$'
license_pattern='^// SPDX-License-Identifier: Apache-2\.0( OR MIT)?$'
missing=()

while IFS= read -r file; do
    copyright_line=$(sed -n '1p' "$file")
    license_line=$(sed -n '2p' "$file")
    if [[ ! $copyright_line =~ $copyright_pattern || ! $license_line =~ $license_pattern ]]; then
        missing+=("$file")
    fi
done < <(git ls-files '*.rs')

if ((${#missing[@]})); then
    printf 'The following Rust files lack the standard copyright header:\n\n'
    printf '  %s\n' "${missing[@]}"
    printf '\nAdd a Runebender copyright line and SPDX license identifier.\n'
    exit 1
fi

echo 'All tracked Rust files have copyright headers.'
