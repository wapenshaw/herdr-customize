// Package fsutil holds the file helpers shared by every writer: atomic
// replacement that keeps symlinks and permissions, and daily config backups.
package fsutil

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// AtomicWrite keeps readers from observing truncated configs, indexes or palettes.
// Resolve an existing symlink so updating a linked config preserves the link.
func AtomicWrite(path string, data []byte, mode os.FileMode) error {
	if info, err := os.Lstat(path); err == nil {
		if info.Mode()&os.ModeSymlink != 0 {
			path, err = filepath.EvalSymlinks(path)
			if err != nil {
				return err
			}
			info, err = os.Stat(path)
			if err != nil {
				return err
			}
		}
		if !info.Mode().IsRegular() {
			return fmt.Errorf("not a regular file: %s", path)
		}
		mode = info.Mode().Perm()
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(path), ".herdr-customize-*")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	defer f.Close()
	if err := f.Chmod(mode); err != nil {
		return err
	}
	if _, err := f.Write(data); err != nil {
		return err
	}
	if err := f.Sync(); err != nil {
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return replaceFile(f.Name(), path)
}

// DailyBackup keeps one copy of the original file per day as path.bak-YYYYMMDD.
// An existing backup for today is never overwritten.
func DailyBackup(path string, data []byte) error {
	bak := path + ".bak-" + time.Now().Format("20060102")
	f, err := os.OpenFile(bak, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	if err == nil {
		_, writeErr := f.Write(data)
		if writeErr == nil {
			writeErr = f.Sync()
		}
		closeErr := f.Close()
		if err := errors.Join(writeErr, closeErr); err != nil {
			cleanupErr := os.Remove(bak)
			return errors.Join(fmt.Errorf("config backup failed: %w", err), cleanupErr)
		}
		return nil
	}
	if !errors.Is(err, os.ErrExist) {
		return fmt.Errorf("config backup failed: %w", err)
	}
	info, statErr := os.Lstat(bak)
	if statErr != nil {
		return fmt.Errorf("check config backup: %w", statErr)
	}
	if !info.Mode().IsRegular() {
		return fmt.Errorf("config backup is not a regular file: %s", bak)
	}
	return nil
}
