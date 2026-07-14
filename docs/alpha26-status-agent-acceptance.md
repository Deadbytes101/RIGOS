RIGOS 0.0.4-ALPHA.26 PHYSICAL ACCEPTANCE
========================================

SOURCE BASE
-----------

    BASE TAG       v0.0.4-alpha.25
    BASE COMMIT    ba02eb7429683550512b703cd4646d4d9ee6a888

Do not move or replace the Alpha.25 tag.

GATE A — FRESH UNCONFIGURED BOOT
--------------------------------

    grep '^VERSION_ID=' /etc/rigos-release
    systemctl is-enabled rigos-status-agent.timer
    systemctl status rigos-status-agent.service rigos-status-agent.timer
    test ! -e /var/lib/rigos/status-agent/ingest.secret
    test ! -e /var/lib/rigos/status-agent/config.env
    systemctl --failed --no-pager
    systemctl is-active rigos-miner.service

Expected:

    VERSION_ID=0.0.4-alpha.26
    timer disabled
    no baked secret or configuration
    no new failed unit
    mining behavior unchanged

GATE B — CONFIGURATION
----------------------

    sudo rig-status-agent configure \
        --server http://STATUS-SERVER:8787 \
        --secret-file /root/rigos-status.secret

    sudo stat -c '%a %U %G %n' \
        /var/lib/rigos/status-agent/ingest.secret \
        /var/lib/rigos/status-agent/config.env \
        /var/lib/rigos/status-agent

Expected:

    secret and env: 600 root root
    state directory: 700 root root
    timer enabled and active

GATE C — SIGNED LIVE INGEST
---------------------------

    sudo rig-status-agent send
    sudo rig-status-agent status --json
    journalctl -u rigos-status-agent.service --since 10m --no-pager

Expected:

    accepted observation
    server source changes to SIGNED LIVE AGENT
    exactly 19 allowlisted components

GATE D — STATUS SERVER OFFLINE
------------------------------

Stop the status server, wait for two timer runs, then inspect:

    systemctl status rigos-status-agent.service
    systemctl --failed --no-pager
    systemctl is-active rigos-miner.service
    sudo rig-status-agent status --json

Expected:

    last-send outcome transport_error
    service result accepted through SuccessExitStatus=75
    miner remains active
    no boot/miner dependency failure

GATE E — REJECTION AND REPLAY
-----------------------------

Use a wrong secret and a captured nonce in controlled tests.

Expected:

    HTTP 401 for bad signature or repeated nonce
    no authenticated history written
    no secret in argv, stdout, stderr or journal

GATE F — REBOOT PERSISTENCE
---------------------------

Record source ID hash, reboot and compare:

    sudo sha256sum /var/lib/rigos/status-agent/source-id
    sudo reboot
    sudo sha256sum /var/lib/rigos/status-agent/source-id

Expected:

    identical source ID
    timer resumes only when configured
    miner behavior remains unchanged

GATE G — PRIVACY
----------------

    sudo rig-status-agent collect > /tmp/rigos-status-observation.json

Search for forbidden data classes. Do not paste real private values into shell
history or reports.

Alpha.26 must not be tagged, called stable or called production-ready until all
physical gates and an unattended soak are recorded against the exact image
hash.
