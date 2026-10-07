// The engraving calibration per laser, fetched once and again after it is
// saved, for the fills that show what it adds.
import { api } from '../api/client';
import type { LaserMode, ScanOffset } from '../api';

class ScanOffsets {
  private tables = $state<Partial<Record<LaserMode, ScanOffset[]>>>({});
  private asked = new Set<LaserMode>();

  /** The measurements for `mode`; empty until they arrive. */
  entries(mode: LaserMode): ScanOffset[] {
    if (!this.asked.has(mode)) {
      this.asked.add(mode);
      api.scanOffsets(mode).then((view) => { this.tables[mode] = view.entries; }).catch(() => this.asked.delete(mode));
    }
    return this.tables[mode] ?? [];
  }

  /** Read again, after the calibration changed. */
  refresh(mode: LaserMode): void { this.asked.delete(mode); void this.entries(mode); }
}

export const scanOffsets = new ScanOffsets();
