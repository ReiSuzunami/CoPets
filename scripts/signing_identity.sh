#!/usr/bin/env bash

copets_signing_identity() {
  printf '%s\n' "${COPETS_SIGNING_IDENTITY:-CoPets Local Signing}"
}
