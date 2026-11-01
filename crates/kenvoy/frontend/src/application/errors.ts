import type { FieldError } from '../domain/types.ts';

interface ErrorOptions { status?: number; code?: string; fields?: FieldError[]; request_id?: string; outcomeUnknown?: boolean }
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fields: FieldError[];
  readonly request_id: string | undefined;
  readonly outcomeUnknown: boolean;
  constructor(message: string, options: ErrorOptions = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = options.status ?? 0;
    this.code = options.code ?? 'KV6000';
    this.fields = options.fields ?? [];
    this.request_id = options.request_id;
    this.outcomeUnknown = options.outcomeUnknown ?? false;
  }
}
