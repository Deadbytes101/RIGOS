RIGOS MULTI-RIG STATUS SETUP
============================

PURPOSE
-------

Register multiple independent RIGOS appliances with the signed public status
service at:

    https://rigos.site/status

Each appliance keeps its own persistent source ID and its own 64-hex HMAC
secret. The public page shows one node card per accepted source ID.

The status service is observation-only. It does not provide remote control.


LIMITS
------

    Registered source keys:    1 through 64
    Public nodes per response: newest 32
    Normal send interval:      30 seconds
    Live threshold:            90 seconds
    Offline threshold:         300 seconds

Never reuse a secret across source IDs.
Never clone an already-configured persistent status-agent state to another rig.


SAFE ORDER OF OPERATIONS
------------------------

1. Disable the status timer on every rig being changed.
2. Read each persistent source ID locally.
3. Generate a separate random 64-hex secret for each source ID.
4. Build one encrypted RIGOS_STATUS_SOURCE_KEYS JSON object.
5. Save it in Cloudflare Pages Production variables and redeploy.
6. Configure each appliance with only its matching secret.
7. Send one manual observation from each appliance.
8. Confirm all expected nodes appear in /api/v1/status.
9. Enable timers one rig at a time.

Do not enable timers while source keys are being rotated. This prevents repeated
rejections and makes failures attributable to one manual send.


STEP 1: DISABLE TIMERS
----------------------

Run on every rig:

    sudo systemctl disable --now rigos-status-agent.timer

Verify:

    systemctl is-enabled rigos-status-agent.timer || true
    systemctl is-active rigos-status-agent.timer || true

Expected:

    disabled
    inactive


STEP 2: READ SOURCE IDS
-----------------------

Run on each appliance:

    sudo rig-status-agent collect |
        python3 -c 'import json,sys; print(json.load(sys.stdin)["sourceId"])'

Alternative after the agent has been configured once:

    sudo cat /var/lib/rigos/status-agent/source-id

Each result must be exactly 64 lowercase hexadecimal characters and must be
unique across the fleet.


STEP 3: GENERATE ONE SECRET PER RIG
-----------------------------------

Windows PowerShell 5.1-compatible helper:

    function New-HexSecret {
        $Bytes = New-Object byte[] 32
        $Rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()

        try {
            $Rng.GetBytes($Bytes)
        }
        finally {
            $Rng.Dispose()
        }

        -join ($Bytes | ForEach-Object { $_.ToString("x2") })
    }

Generate and save one file per appliance. Do not print the values:

    $Rig01Secret = New-HexSecret
    $Rig02Secret = New-HexSecret

    [System.IO.File]::WriteAllText(
        "D:\TECHNICAL\rigos-status-rig01.secret",
        $Rig01Secret,
        [System.Text.Encoding]::ASCII
    )

    [System.IO.File]::WriteAllText(
        "D:\TECHNICAL\rigos-status-rig02.secret",
        $Rig02Secret,
        [System.Text.Encoding]::ASCII
    )

Validate without displaying secrets:

    foreach ($Secret in @($Rig01Secret, $Rig02Secret)) {
        if ($Secret -notmatch '^[a-f0-9]{64}$') {
            throw "Invalid generated secret"
        }

        if ($Secret -eq ("0" * 64)) {
            throw "All-zero secret rejected"
        }
    }

    if ($Rig01Secret -eq $Rig02Secret) {
        throw "Duplicate rig secrets rejected"
    }


STEP 4: BUILD THE SOURCE REGISTRY
---------------------------------

Example PowerShell structure:

    $Registry = [ordered]@{
        "<rig01-64-hex-source-id>" = $Rig01Secret
        "<rig02-64-hex-source-id>" = $Rig02Secret
    } | ConvertTo-Json -Compress

    Set-Clipboard -Value $Registry

Do not print $Registry. It contains every status-agent secret.

Cloudflare Pages Production variable:

    Type:           Secret
    Variable name:  RIGOS_STATUS_SOURCE_KEYS
    Value:          <paste the compact JSON object>

RIGOS_STATUS_SOURCE_KEYS is authoritative when present. Redeploy Production
after adding or changing it.

The JSON shape is:

    {
      "<source-id-a>": "<secret-a>",
      "<source-id-b>": "<secret-b>"
    }


STEP 5: COPY THE MATCHING SECRET
--------------------------------

Copy only the matching secret to each appliance:

    scp D:\TECHNICAL\rigos-status-rig01.secret \
        rigosadmin@<rig01-ip>:/home/rigosadmin/status.secret

    scp D:\TECHNICAL\rigos-status-rig02.secret \
        rigosadmin@<rig02-ip>:/home/rigosadmin/status.secret

On each rig:

    sudo install \
        -o root \
        -g root \
        -m 0600 \
        /home/rigosadmin/status.secret \
        /root/rigos-status.secret

    rm -f /home/rigosadmin/status.secret

    sudo rig-status-agent configure \
        --server https://rigos.site \
        --secret-file /root/rigos-status.secret \
        --no-start

    sudo rm -f /root/rigos-status.secret
    sudo systemctl disable --now rigos-status-agent.timer

The secret remains in the agent's protected persistent configuration. The
temporary copy is removed.


STEP 6: MANUAL ACCEPTANCE
-------------------------

Send from one rig at a time:

    sudo rig-status-agent send
    sudo rig-status-agent status --json | python3 -m json.tool

Required result:

    configured:        true
    server:            https://rigos.site
    lastSend.outcome:  accepted
    lastSend.detail:   Signed observation accepted
    timer:             disabled / inactive

Failure meanings:

    HTTP 401 unknown_source
        The source ID is absent from RIGOS_STATUS_SOURCE_KEYS.

    HTTP 401 signature_mismatch
        The appliance secret does not match the registry value for that source.

    HTTP 422 invalid_observation
        Signature verification passed, but the observation contract failed.

    HTTP 503 source_registry_unavailable
        The Cloudflare registry is absent or malformed.


STEP 7: VERIFY THE FLEET
------------------------

Run from any RIGOS appliance:

    curl -fsS https://rigos.site/api/v1/status |
    python3 -c '
    import json
    import sys

    document = json.load(sys.stdin)
    print("NODE_COUNT", document["nodeCount"])

    for node in sorted(document["nodes"], key=lambda item: item["nodeId"]):
        print(
            node["nodeId"],
            node["connection"],
            node["systemState"],
            len(node["components"]),
        )
    '

Every accepted rig should have a unique 12-character public node ID, derived
from the first 12 characters of its full source ID.

Expected healthy row:

    <node-id> live operational 19


STEP 8: ENABLE AUTOMATIC SENDS
------------------------------

Enable one rig, wait for an automatic accepted send, then continue to the next:

    sudo systemctl enable --now rigos-status-agent.timer
    sleep 40

    sudo rig-status-agent status --json | python3 -m json.tool
    sudo systemctl list-timers \
        --all \
        rigos-status-agent.timer \
        --no-pager

Required state:

    service result:     success
    last send:          accepted
    timer unit state:   enabled
    timer active state: active
    timer substate:     waiting
    next run:           present in systemctl list-timers


ADDING ANOTHER RIG
------------------

1. Flash from the original image, not from another configured USB device.
2. Boot and verify hardware health.
3. Confirm the new source ID is unique.
4. Disable its timer.
5. Generate a new secret.
6. Add one new source-id-to-secret entry to RIGOS_STATUS_SOURCE_KEYS.
7. Redeploy Cloudflare Production.
8. Configure and manually send from the new rig.
9. Confirm nodeCount increased by one.
10. Enable its timer.

Do not remove existing registry entries while adding a rig. Removing one makes
that appliance fail closed with HTTP 401 unknown_source.


KEY ROTATION
------------

Rotate one appliance at a time:

1. Disable its timer.
2. Generate a replacement secret.
3. Replace only that source ID's value in RIGOS_STATUS_SOURCE_KEYS.
4. Redeploy.
5. Reconfigure that appliance with the same replacement secret.
6. Send manually and require accepted.
7. Re-enable its timer.

A stale or offline card can remain visible from the last accepted observation.
A failed signature never overwrites stored evidence.


RANDOMX MSR EVIDENCE COMPATIBILITY
----------------------------------

The ingest path accepts both bounded forms emitted by RIGOS Alpha.26 hardware:

    schema/outcome evidence
        rigos.randomx-msr-status/v1

    exact systemd unit evidence
        rigos-randomx-msr.service

The unit form is accepted only for that exact service name and only for the
allowlisted state, result and fact fields. Arbitrary unit names or facts remain
rejected.


SECURITY RULES
--------------

    one source ID = one secret
    never commit secrets
    never paste secrets into issues, logs or chat
    keep Cloudflare registry type set to Secret
    use mode 0600 for temporary secret files on appliances
    remove temporary transferred files after configuration
    disable timers during registry changes
    require manual accepted sends before automatic operation

No magic. No guessing. Register the source, sign the bytes and read what the
server accepted.
