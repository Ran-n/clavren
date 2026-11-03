[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/03 04:54:17.375058 )
[//]: # (+ Revised: 	2026/11/03 15:25:08.004507 )
[//]: # ( ---------------------------------------------------------------------- )

# Backup and recovery

The database and `master.key` are useless without each other: the licence keys and the admins'
TOTP secrets in the database are encrypted with keys derived from `master.key`. Losing either one
breaks every customer's licence. Every backup holds both.

---

## What a backup is

`clavren-backup.timer` runs `clavren-backup.sh` every night at about 03:15 (a run missed while the
Pi was off happens at the next boot). Each run writes to `CLAVREN_BACKUP_DIR`:

| File | Contents |
|------|----------|
| `clavren-20270115T031502Z.tar` | `clavren.dump` (`pg_dump`, custom format) and `master.key` |
| `clavren-20270115T031502Z.tar.b3` | Its BLAKE3 checksum (`b3sum` format) |

With `CLAVREN_BACKUP_AGE_RECIPIENT` set, the archive is encrypted with
[age](https://age-encryption.org) and ends in `.tar.age`; keep the private key **off the Pi**.
Without it, an archive holds the master key in the clear: treat the backup drive like the Pi
itself.

The script:

- refuses to run when the backup directory is on the system drive (the drive isn't mounted), so it
  never fills the SD card;
- writes into a work folder on the backup drive and moves the finished archive into place, so a
  half-written archive never looks like a backup;
- checks the checksum it wrote, records the run in the database (the dashboard's Overview and
  Backups pages), and keeps the newest `CLAVREN_BACKUP_KEEP` archives (14);
- exits non-zero on any failure: it shows in `systemctl --failed` and
  `journalctl -u clavren-backup`. The Overview warns when the last backup is more than 48 hours
  old.

Run one by hand (before every upgrade):

```sh
sudo systemctl start clavren-backup
journalctl -u clavren-backup -n 5
```

Copy an archive somewhere other than the Pi now and then: a USB drive plugged into the Pi doesn't
survive a fire or a theft.

---

## No backup drive yet

Until a drive is mounted, the nightly backup fails and the Overview warns. `master.key` then
exists only on the SD card, so make a backup by hand and keep it on another computer, now and
after issuing keys:

1. On the Pi, back up into memory (`/dev/shm`). It never touches the SD card, the script accepts
   it as another drive, and it is gone at the next reboot:

   ```sh
   sudo install -d -o clavren -g clavren -m 0700 /dev/shm/clavren-backup
   sudo runuser -u clavren -- bash -c 'set -a; . /etc/clavren/clavren.env; CLAVREN_BACKUP_DIR=/dev/shm/clavren-backup; exec /usr/local/lib/clavren/clavren-backup.sh'
   sudo cp /dev/shm/clavren-backup/clavren-* ~ && sudo chown "$USER" ~/clavren-*
   ```

2. On your computer, copy it over and check it:

   ```sh
   scp 'your-pi:clavren-*' .
   b3sum --check clavren-*.b3
   ```

3. On the Pi, delete both copies:

   ```sh
   rm ~/clavren-*
   sudo rm -rf /dev/shm/clavren-backup
   ```

The run is recorded like any other, so the Overview shows it. The archive holds `master.key`
unencrypted unless `CLAVREN_BACKUP_AGE_RECIPIENT` is set: keep it somewhere private.

---

## Checking a backup

```sh
cd /mnt/clavren-backups
b3sum --check clavren-20270115T031502Z.tar.b3
tar -tf clavren-20270115T031502Z.tar   # clavren.dump, master.key
```

The test `a_backup_restores_into_an_empty_database` (`crates/clavren-server/tests/backup_restore.rs`)
makes a dump the same way, restores it into an empty database, and checks that an activated
computer still checks in, the key can be revealed with the same master key, and customer data is
intact.

---

## Restoring

Onto a new Pi (after a dead SD card, say), or onto the same one after a mistake.

1. **New Pi only**: install Clavren as in [deployment.md](deployment.md), steps 1 to 3, with
   either option. The install starts Clavren with a new, empty database and a new `master.key`;
   the restore replaces both. Pick the archive to restore from **before** the problem, not the
   empty backup the installation just made.

   **Same Pi**: skip this step.

2. Restore the archive:

   ```sh
   sudo /usr/local/lib/clavren/clavren-restore.sh /mnt/clavren-backups/clavren-20270115T031502Z.tar
   # encrypted: add the age identity file
   sudo /usr/local/lib/clavren/clavren-restore.sh archive.tar.age ~/clavren-backup-identity.txt
   ```

   It checks the checksum, decrypts if needed, stops `clavren`, replaces the database contents
   (`pg_restore --clean`, in one transaction), installs `master.key` (clavren, 0600), and starts
   `clavren` again.

3. Check:

   ```sh
   systemctl status clavren
   curl -s http://127.0.0.1:8080/health/ready
   ```

   Sign in to the dashboard (same admins, same authenticator apps), reveal a key, and confirm on
   the Overview that computers are checking in. An application that was activated before the
   backup keeps working: its activation, device key and licence are in the restored database.

4. Enable the timer if this is a new Pi (`sudo systemctl enable --now clavren-backup.timer`) and
   make a fresh backup.

### What a restore loses

Everything after the backup: licences issued, activations, resets and revocations. A computer that
activated after the backup is unknown to the restored server and is refused at its next check;
issue the customer a new key or reset theirs. A licence revoked after the backup works again until
it is revoked again.

### Without `master.key`

Clavren refuses to start when the database has licences or admins but `master.key` is missing; it
never makes a replacement. There is no way to recover the keys without it. The only way forward is
a new installation and new keys for every customer.
