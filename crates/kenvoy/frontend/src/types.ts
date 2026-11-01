/** 浏览器协议只含公开投影；不含宿主路径、源码或服务端函数实现。 */
export type JsonScalar = string | number | boolean | null;
export type UploadedFile = { id: string; name: string; content_type: string; size: number };
export interface FileConstraints { accept: string[]; max_size: number; max_files: number }
export type FieldValue = JsonScalar | UploadedFile[];
export type JsonValue = JsonScalar | JsonValue[] | { [key: string]: JsonValue };
export type Role = 'anonymous' | 'user' | 'admin';
export type FieldKind = 'text' | 'textarea' | 'number' | 'boolean' | 'date' | 'datetime' | 'select' | 'file';
export interface SelectOption { value: string; label: string; when?: Record<string, JsonScalar> }
export interface OptionsSource { resource: string; value: string; label: string; filters: Record<string, string>; permission: Role }
export interface OptionsQuery { dependencies: Record<string, JsonScalar>; search?: string; page?: number; page_size?: number; selected?: string }
export interface OptionsResult { data: SelectOption[]; page: number; page_size: number; total: number; generation: number; selected: SelectOption | null }
export type SystemComponentName = FieldKind | 'image' | 'form' | 'table' | 'group' | 'page';
export interface FieldDefinition {
  name: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  integer: boolean;
  min: number | null;
  max: number | null;
  max_length: number | null;
  unique: boolean;
  options: SelectOption[];
  options_source?: OptionsSource | null;
  upload?: FileConstraints | null;
  /** 缺失与显式 null 的默认值必须区分。 */
  default?: JsonValue;
}
export interface ResourceDefinition { name: string; fields: FieldDefinition[] }
export type PropValue = { kind: 'literal'; value: JsonValue } | { kind: 'module'; value: string };
export type ComponentDefinition =
  | { type: 'form'; id: string; resource: string; mode: 'create' | 'edit'; fields: string[]; labels: Record<string, string> }
  | { type: 'table'; resource: string; columns: string[] }
  | { type: 'image'; src: string; alt: string; width?: number | null; height?: number | null; fit: 'contain' | 'cover' }
  | { type: 'group'; label: string | null; children: ComponentDefinition[] }
  | { type: 'custom'; module: string; props: Record<string, PropValue>; children: ComponentDefinition[] }
  | { type: 'text'; value: string };
export interface PageDefinition { path: string; title: string; permission: Role; components: ComponentDefinition[] }
export type NavigationItem =
  | { type: 'link'; label: string; path: string }
  | { type: 'group'; label: string | null; items: NavigationItem[] };
export interface NavigationDefinition { label: string | null; items: NavigationItem[] }
export type ExecutionEnvironment = 'client' | 'server' | 'shared';
export interface ActionDefinition { name: string; runtime: ExecutionEnvironment; permission: Role; handler?: string }
export type Operation = 'create' | 'list' | 'read' | 'update' | 'delete' | 'execute';
export type PermissionTarget = { kind: 'resource' | 'action'; name: string };
export interface PermissionDefinition { target: PermissionTarget; operation: Operation; fields: string[]; form: string | null }
export interface ApplicationDescription {
  format_version: 1;
  name: string;
  generation: number;
  /** 只有开发宿主主动开放状态端点；缺省及 false 均不轮询。 */
  development?: boolean;
  role: Role;
  pages: PageDefinition[];
  resources: ResourceDefinition[];
  navigation: NavigationDefinition[];
  actions: ActionDefinition[];
  permissions: PermissionDefinition[];
  component_overrides?: Partial<Record<SystemComponentName, string>>;
}
export interface DevelopmentStatus {
  revision: number;
  generation: number;
  state: 'ready' | 'building' | 'error';
  error?: string;
  reload: boolean;
}
export interface UserView { id: number; username: string; role: 'user' | 'admin' }
export interface SessionView { user: UserView | null; csrf_token: string; expires_at: number; generation: number }
export interface FieldError { code: string; field: string | null; message: string }
export interface RecordData { [key: string]: FieldValue; id: number; created_at: string; updated_at: string }
export interface ListResult { data: RecordData[]; page: number; page_size: number; total: number; generation: number }
export interface ListQuery {
  page?: number;
  page_size?: number;
  search?: string;
  sort?: string;
  descending?: boolean;
  /** 当前 HTTP 协议只开放等值筛选，每个字段只出现一次。 */
  filters?: Record<string, JsonScalar>;
}
export type Field = FieldDefinition;
export type Resource = ResourceDefinition;
export type ComponentNode = ComponentDefinition;
export type Page = PageDefinition;
export type RecordValue = FieldValue;
export type DataRecord = RecordData;
