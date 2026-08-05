## Usage

In Linux, you need to install `pcscd`, `pcsclite` and `libcurl`.

APDU and HTTP interfaces of lpac has several backends, you need to specify `$LPAC_APDU` and `$LPAC_HTTP` environment variables to interface library path. If not specified, it will use `pcsc` and `curl`. See also [environment variables](ENVVARS.md).

## CLI

### Command format

```plain
lpac <subcommand> [subcommand] [parameters]
  subcommand:
    chip          View and manage information about your eUICC card itself
    profile       Manage your eUICC profiles
    notification  Manage notifications within your eUICC card
    driver        View libXXXXinterface info
```

### Return value

The return contents of lpac instructions are in newline-delimited JSON. Commands may emit zero or more `progress` events followed by exactly one final `lpa` event.

Final event format:

```jsonc
{
  "type": "lpa",
  "payload": {
    "code": 0,
    "message": "success",
    "data": {}
  }
}
```

- `code == 0` indicates success.
- Other codes indicate an error.
- `data` contains the command result or error detail.

### Subcommand

#### chip

View the EID, default SM-DP+ server and SM-DS server of eUICC. `euicc_info2` is also supported.

```plain
lpac chip <subcommand> [parameters]
  subcommand:
    info         View information about your eUICC card itself
    defaultsmdp  Modify the default SM-DP+ server address
    purge        Reset the eUICC and clear all profiles. Use with caution.
```

#### profile

Profile management includes list, nickname, enable, disable, delete, download, and discovery operations.

```plain
lpac profile <subcommand> [parameters]
  subcommand:
    list
    nickname
    enable
    disable
    delete
    download
    discovery
```

##### Profile download

Profile download requires access to the SM-DP+ server. Supported options:

- `-s`: SM-DP+ server, optional.
- `-m`: Matching ID, optional.
- `-c`: Confirmation code, optional. Use `-c -` to read one line from stdin.
- `-i`: IMEI, optional.
- `-a`: Full QR activation string such as `LPA:1$<sm-dp+ domain>$<matching id>`. This takes precedence over `-s` and `-m`. Use `-a -` to read one line from stdin.
- `-p`: Interactive preview mode.

Examples:

```bash
./lpac profile download -s rsp.example -m "MATCHING-ID"
./lpac profile download -a 'LPA:1$rsp.example$MATCHING-ID'
```

To keep the activation code out of argv and process listings:

```bash
printf '%s\n' 'LPA:1$rsp.example$MATCHING-ID' \
  | ./lpac profile download -a -
```

With a confirmation code, stdin contains the activation code followed by the confirmation code, one line each:

```bash
printf '%s\n%s\n' 'LPA:1$rsp.example$MATCHING-ID' 'CONFIRMATION-CODE' \
  | ./lpac profile download -a - -c -
```

Do not place secrets in shell history, debug logs, or environment variables.

##### Discovery

Discovery queries the SM-DS server. It supports:

- `-s`: SM-DS server; defaults to `lpa.ds.gsma.com`.
- `-i`: optional IMEI.

#### notification

Use `lpac notification` to list, process, remove, replay, or dump eUICC notifications. Refer to command help for the available options.

#### driver

Use `lpac driver` to inspect available APDU and HTTP interface libraries.
