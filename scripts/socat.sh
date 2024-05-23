#!/usr/bin/env bash
set -e

# shellcheck disable=SC2034
script_name=$(basename "$0")
script_dir="$( cd "$(dirname "$0")" || exit ; pwd -P )"
project_dir=$(realpath "${script_dir}/..")

usage() {
    printf "usage: %s [OPTS]\\n" "${script_name}"
    printf "OPTS:\\n"
    printf "    -u | --uart                     Serial uart device to use\\n"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "${1}" in
        -u|--uart) cli_uart="${2}"; shift ;;
        -h*|--help*|*) usage ;;
    esac
    shift
done

socat -x -v PTY,link=${project_dir}/debug-serial,rawer,echo=0 ${cli_uart},rawer,echo=0,ispeed=115200,ospeed=115200,nonblock
