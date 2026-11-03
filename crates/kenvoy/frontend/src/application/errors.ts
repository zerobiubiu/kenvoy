import type { FieldError } from '../domain/types.ts';
import type { MessageParams } from '../domain/i18n/model.ts';

interface ErrorOptions { status?: number; code?: string; fields?: FieldError[]; request_id?: string; outcomeUnknown?: boolean; message_key?: string; params?: MessageParams }
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fields: FieldError[];
  readonly request_id: string | undefined;
  readonly outcomeUnknown: boolean;
  readonly message_key: string | undefined;
  readonly params: MessageParams;
  constructor(message: string, options: ErrorOptions = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = options.status ?? 0;
    this.code = options.code ?? 'KV6000';
    this.fields = options.fields ?? [];
    this.request_id = options.request_id;
    this.outcomeUnknown = options.outcomeUnknown ?? false;
    this.message_key = options.message_key;
    this.params = options.params ?? {};
  }
}
