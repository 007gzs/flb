export interface Certificate {
  id: string
  name: string
  certPem: string
  keyPem: string
  notAfter?: string | null
  autoIssued: boolean
  createdAt: string
}

export interface DnsProvider {
  id: string
  name: string
  kind: 'aliyun' | 'wanwang' | 'godaddy'
  accessKey: string
  accessSecret: string
}

export interface Domain {
  id: string
  name: string
  mode: 'manual' | 'acme'
  certId?: string | null
  challenge?: 'http01' | 'dns01' | null
  dnsProviderId?: string | null
  status: string
  lastError?: string | null
  expiresAt?: string | null
  createdAt: string
}

export interface UpstreamServer {
  address: string
  port?: number | null
  weight: number
  protocol?: 'http' | 'https'
  verifyTls?: boolean
}

export interface Upstream {
  id: string
  name: string
  protocol?: 'http' | 'https'
  verifyTls?: boolean
  sni?: string | null
  servers: UpstreamServer[]
}

export interface HeaderRewrite {
  name: string
  value: string
}

export interface RouteRule {
  id?: string
  pattern: string
  matchType: 'prefix' | 'regex'
  rewriteUri?: string | null
  methods: string[]
  requestHeaders: HeaderRewrite[]
  responseHeaders: HeaderRewrite[]
  upstreamId: string
}

export interface Host {
  id: string
  hostname: string
  protocol: string
  httpsEnabled: boolean
  certId?: string | null
  forceHttps: boolean
  defaultUpstreamId: string
  routes: RouteRule[]
}

export interface StreamConfig {
  id: string
  name: string
  listenPort: number
  protocol: 'tcp' | 'udp'
  targetIp: string
  targetPort: number
}

export interface Stats {
  certs: number
  dnsProviders: number
  domains: number
  upstreams: number
  hosts: number
  streams: number
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    headers: { 'Content-Type': 'application/json', ...(init?.headers || {}) },
    ...init,
  })
  if (res.status === 204)
    return undefined as T
  const text = await res.text()
  const data = text ? JSON.parse(text) : null
  if (!res.ok)
    throw new Error(data?.message || res.statusText)
  return data as T
}

export const api = {
  stats: () => request<Stats>('/api/stats'),
  certs: {
    list: () => request<Certificate[]>('/api/certs'),
    create: (body: Partial<Certificate>) => request<Certificate>('/api/certs', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: Partial<Certificate>) => request<Certificate>(`/api/certs/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    remove: (id: string) => request<void>(`/api/certs/${id}`, { method: 'DELETE' }),
  },
  dns: {
    list: () => request<DnsProvider[]>('/api/dns-providers'),
    create: (body: Partial<DnsProvider>) => request<DnsProvider>('/api/dns-providers', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: Partial<DnsProvider>) => request<DnsProvider>(`/api/dns-providers/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    remove: (id: string) => request<void>(`/api/dns-providers/${id}`, { method: 'DELETE' }),
  },
  domains: {
    list: () => request<Domain[]>('/api/domains'),
    create: (body: Partial<Domain>) => request<Domain>('/api/domains', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: Partial<Domain>) => request<Domain>(`/api/domains/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    remove: (id: string) => request<void>(`/api/domains/${id}`, { method: 'DELETE' }),
    renew: (id: string) => request<Domain>(`/api/domains/${id}/renew`, { method: 'POST' }),
  },
  upstreams: {
    list: () => request<Upstream[]>('/api/upstreams'),
    create: (body: Partial<Upstream>) => request<Upstream>('/api/upstreams', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: Partial<Upstream>) => request<Upstream>(`/api/upstreams/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    remove: (id: string) => request<void>(`/api/upstreams/${id}`, { method: 'DELETE' }),
  },
  hosts: {
    list: () => request<Host[]>('/api/hosts'),
    create: (body: Partial<Host>) => request<Host>('/api/hosts', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: Partial<Host>) => request<Host>(`/api/hosts/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    remove: (id: string) => request<void>(`/api/hosts/${id}`, { method: 'DELETE' }),
  },
  streams: {
    list: () => request<StreamConfig[]>('/api/streams'),
    create: (body: Partial<StreamConfig>) => request<StreamConfig>('/api/streams', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: Partial<StreamConfig>) => request<StreamConfig>(`/api/streams/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    remove: (id: string) => request<void>(`/api/streams/${id}`, { method: 'DELETE' }),
  },
}
