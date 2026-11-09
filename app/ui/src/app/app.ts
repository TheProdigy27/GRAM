import { Component, computed, inject, resource } from '@angular/core';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatToolbarModule } from '@angular/material/toolbar';
import { TauriService } from './core/tauri.service';

@Component({
  selector: 'app-root',
  imports: [MatCardModule, MatIconModule, MatProgressBarModule, MatToolbarModule],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  private readonly tauri = inject(TauriService);

  protected readonly ping = resource({ loader: () => this.tauri.ping() });

  protected readonly pingError = computed(() => {
    const err = this.ping.error();
    return err ? (err instanceof Error ? err.message : String(err)) : null;
  });
}
