// 连接器目录：curated 的外部数据源元数据。
// 每项最终物化成一条带 Authorization header 的 ChatMcpServer（见 ConnectorsPanel）。
// authKind:
//   - 'token'：用户粘贴 PAT/API key → headers.Authorization = 'Bearer <token>'（Phase A 已支持）。
//   - 'oauth'：OAuth 2.1 + PKCE 授权（Phase B 实现；Phase A 卡片连接按钮禁用）。

export type ConnectorAuthKind = 'oauth' | 'token'

export type ConnectorCatalogEntry = {
  id: string
  name: string
  /** 双语简介。 */
  description: { zh: string; en: string }
  /** NavIcons 之外的图标键，用于卡片渲染（见 ConnectorsPanel 的图标映射）。 */
  iconKey: string
  /** MCP（streamable_http）端点 URL。 */
  url: string
  authKind: ConnectorAuthKind
  /** 数据是否经第三方中转（Composio/Rube 等聚合服务）。 */
  composio?: boolean
  /** token 输入框的占位提示（双语）。 */
  tokenHint?: { zh: string; en: string }
  /** 详情面板「概览」要点（双语 bullet 列表）。 */
  overview?: { zh: string[]; en: string[] }
  /** 官网链接（详情面板「链接」区）。 */
  website?: string
  /** 支持/帮助链接（详情面板「链接」区）。 */
  support?: string
  /** 开发者/提供方名称（详情面板「开发者」区）。 */
  developer?: string
}

export const CONNECTOR_CATALOG: ConnectorCatalogEntry[] = [
  {
    id: 'notion',
    name: 'Notion',
    description: {
      zh: '读取与检索 Notion 页面、数据库。',
      en: 'Read and search Notion pages and databases.',
    },
    iconKey: 'notion',
    url: 'https://mcp.notion.com/mcp',
    authKind: 'oauth',
    overview: {
      zh: [
        '检索与读取 Notion 页面、数据库内容。',
        '按关键词在工作区内全文搜索。',
        '通过 OAuth 授权，无需手动管理 token。',
      ],
      en: [
        'Search and read Notion pages and database content.',
        'Full-text search across your workspace by keyword.',
        'Authorize via OAuth — no manual token management.',
      ],
    },
    website: 'https://www.notion.so',
    support: 'https://developers.notion.com',
    developer: 'Notion Labs, Inc.',
  },
  {
    id: 'github',
    name: 'GitHub',
    description: {
      zh: '访问仓库、Issue、PR 与代码搜索（使用 Personal Access Token）。',
      en: 'Access repos, issues, PRs, and code search (via Personal Access Token).',
    },
    iconKey: 'github',
    url: 'https://api.githubcopilot.com/mcp/',
    authKind: 'token',
    tokenHint: {
      zh: '粘贴 GitHub Personal Access Token（PAT）',
      en: 'Paste a GitHub Personal Access Token (PAT)',
    },
    overview: {
      zh: [
        '访问仓库、Issue、Pull Request 与代码搜索。',
        '读取文件内容、提交记录与分支信息。',
        '使用 Personal Access Token 鉴权，权限由 token scope 决定。',
      ],
      en: [
        'Access repositories, issues, pull requests, and code search.',
        'Read file contents, commit history, and branch info.',
        'Authenticated via a Personal Access Token; scope-limited.',
      ],
    },
    website: 'https://github.com',
    support: 'https://docs.github.com/copilot',
    developer: 'GitHub, Inc.',
  },
  {
    id: 'composio',
    name: 'Composio',
    description: {
      zh: '通过 Composio 聚合接入 Gmail、Drive、Outlook 等长尾服务。数据经第三方中转，请确认 MCP 端点与 token。',
      en: 'Reach long-tail services (Gmail, Drive, Outlook…) through Composio. Data is relayed by a third party; confirm the MCP endpoint and token.',
    },
    iconKey: 'composio',
    // 占位端点：用户需自行填入/确认其 Composio (Rube) MCP 端点。
    url: 'https://mcp.composio.dev/mcp',
    authKind: 'token',
    composio: true,
    tokenHint: {
      zh: '粘贴 Composio API key / token',
      en: 'Paste your Composio API key / token',
    },
    overview: {
      zh: [
        '通过 Composio 聚合接入 Gmail、Drive、Outlook 等长尾服务。',
        '一个 token 复用多家服务，统一 MCP 端点。',
        '数据经第三方中转，请确认 MCP 端点与 token 来源可信。',
      ],
      en: [
        'Reach long-tail services (Gmail, Drive, Outlook…) through Composio.',
        'One token spans many services via a unified MCP endpoint.',
        'Data is relayed by a third party; verify the endpoint and token.',
      ],
    },
    website: 'https://composio.dev',
    support: 'https://docs.composio.dev',
    developer: 'Composio',
  },
]
