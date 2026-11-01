import type { ApplicationDescription, DevelopmentStatus, JsonValue, ListQuery, ListResult, OptionsQuery, OptionsResult, RecordData, SessionView, UploadedFile } from '../domain/types.ts';

/** 应用使用同一会话、代际和资源协议，不依赖具体 Fetch 适配器。 */
export interface Client {
  readonly session: SessionView | null;
  getSession(): Promise<SessionView>;
  getApplication(): Promise<ApplicationDescription>;
  getDevelopment(): Promise<DevelopmentStatus>;
  login(username: string, password: string): Promise<SessionView>;
  logout(): Promise<SessionView>;
  list(resource: string, query?: ListQuery): Promise<ListResult>;
  get(resource: string, id: number): Promise<RecordData>;
  options(resource: string, field: string, query: OptionsQuery, form?: string): Promise<OptionsResult>;
  create(resource: string, data: Record<string, JsonValue>, form?: string): Promise<RecordData>;
  update(resource: string, id: number, data: Record<string, JsonValue>): Promise<RecordData>;
  delete(resource: string, id: number): Promise<void>;
  upload(resource: string, field: string, file: File, form?: string, mode?: 'create' | 'update'): Promise<UploadedFile>;
  removeUpload(id: string): Promise<void>;
  executeAction(name: string, args?: JsonValue[]): Promise<JsonValue>;
}
