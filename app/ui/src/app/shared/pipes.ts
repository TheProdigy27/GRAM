import { Pipe, type PipeTransform } from '@angular/core';
import { count, ctxK, gb, params, tokS } from './format';
import { quantName } from './labels';

/** Octets → « 15,9 Go ». */
@Pipe({ name: 'gb' })
export class GbPipe implements PipeTransform {
  transform(bytes: number): string {
    return gb(bytes);
  }
}

/** Vitesse → « ~38 ». */
@Pipe({ name: 'tokS' })
export class TokSPipe implements PipeTransform {
  transform(t: number): string {
    return tokS(t);
  }
}

/** Contexte → « 32k ». */
@Pipe({ name: 'ctx' })
export class CtxPipe implements PipeTransform {
  transform(tokens: number): string {
    return ctxK(tokens);
  }
}

/** Compteur → « 1,2 M ». */
@Pipe({ name: 'count' })
export class CountPipe implements PipeTransform {
  transform(n: number): string {
    return count(n);
  }
}

/** Paramètres → « 30 Md (3,3 Md actifs) ». */
@Pipe({ name: 'params' })
export class ParamsPipe implements PipeTransform {
  transform(total: number, active: number | null): string {
    return params(total, active);
  }
}

/** Quantification → son nom affiché, sans le nom du modèle (« Underdog-Saluki-Swift-1.5-IQ2 »). */
@Pipe({ name: 'quant' })
export class QuantNamePipe implements PipeTransform {
  transform(variant: { label: string; repoId: string }): string {
    return quantName(variant);
  }
}
