/*
 * This file is automatically managed by CSICK. Please refrain from modifying
 * its contents.
 *
 * ***** COMMON ISSUES AND FIXES *****
 * If you are encountering errors from a missing include, add it in
 * csick.json's additional_includes field.
 * If you receive UNKNOWN_TYPE on the Rust end, add a custom mapping in
 * csick.json's "additional_mappings" field, e.g. {"MyClass": "u64"}.
 */

#pragma once
#define CSICK __attribute__((annotate("CSICK")))
#define CSICKD(UID)                                                            \
  __attribute__((annotate("CSICKD"))) __attribute__((annotate("CSK_" #UID)))