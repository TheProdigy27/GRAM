import { GIB, HARDWARE } from '../../../testing/fixtures';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import { type BannerPart, cpuParts, diskParts, gpuParts, ramParts } from './hardware-banner';

const join = (parts: BannerPart[]) =>
  parts.map((p) => (p.source ? `${p.text} (${p.source})` : p.text)).join(' · ');

describe('bandeau matériel', () => {
  it('machine de développement', () => {
    expect(join(cpuParts(HARDWARE))).toBe(
      'AMD Ryzen 7 9800X3D 8-Core Processor · 8c/16t · AVX-512',
    );
    expect(join(ramParts(HARDWARE))).toBe('64 Go DDR5-6000 · 2 canaux (estimée) · ≈96 Go/s');
    expect(join(gpuParts(HARDWARE))).toBe(
      'NVIDIA GeForce RTX 5080 · 16 Go · ≈960 Go/s (table des GPU) · CUDA 12.9 · Vulkan',
    );
    expect(join(diskParts(HARDWARE))).toBe('C: 210 Go libres');
  });

  it('portable sans GPU dédié, RAM de type inconnu', () => {
    const p: HardwareProfile = {
      ...HARDWARE,
      cpu: { ...HARDWARE.cpu, avx512: false },
      ram: {
        ...HARDWARE.ram,
        installed: null,
        total: 15.7 * GIB,
        kind: 'unknown',
        speedMts: 3200,
        channels: 1,
        channelsSource: 'detected',
        bandwidthGbs: null,
      },
      gpu: { ...HARDWARE.gpu!, name: 'Intel(R) Iris(R) Xe Graphics', integrated: true },
      backends: { cuda: false, cudaDriverVersion: null, vulkan: true },
    };
    expect(join(cpuParts(p))).toContain('AVX2');
    expect(join(ramParts(p))).toBe('16 Go 3200 MT/s · 1 canal');
    expect(join(gpuParts(p))).toBe(
      'Intel(R) Iris(R) Xe Graphics · intégré : compté comme du CPU (mémoire partagée)',
    );
    expect(join(gpuParts({ ...p, gpu: null }))).toBe('Aucun GPU : calcul sur le CPU');
  });

  it('bande passante corrigée à la main', () => {
    const p = {
      ...HARDWARE,
      gpu: { ...HARDWARE.gpu!, bandwidthGbs: 900, bandwidthSource: 'manual' as const },
    };
    expect(join(gpuParts(p))).toContain('≈900 Go/s (corrigée)');
  });
});
