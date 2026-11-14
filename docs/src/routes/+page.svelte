<script lang="ts">
	import { Surface, Tabs, TextInput } from 'bref-ui'
	import { Activity, ArrowDownToLine, ArrowRight, ArrowUpRight, BookOpen, Bot, Check, ChevronDown, ChevronRight, Copy, Search, Star, Wallet, X } from 'lucide-svelte'
	import hljs from 'highlight.js/lib/core'
	import json from 'highlight.js/lib/languages/json'
	import bash from 'highlight.js/lib/languages/bash'
	import type { PageData } from './$types'

	let { data }: { data: PageData } = $props()
	let query = $state('')
	let selectedId = $state<string>()
	let selectedSchema = $state('')
	let activeServer = $state<string>()
	let expandedTags = $state<Record<string, boolean>>({})
	let copied = $state(false)
	let currentView = $state<'introduction' | 'api'>('introduction')
	hljs.registerLanguage('json', json)
	hljs.registerLanguage('bash', bash)
	const serverTabs = $derived(data.spec.servers.map((server, index) => ({
		id: server.url,
		label: index === data.spec.servers.length - 1 ? 'Production' : 'Local',
		icon: index === data.spec.servers.length - 1 ? ('cloud' as const) : ('terminal' as const),
	})))
	const selectedOperationId = $derived(selectedId ?? data.operations[0]?.id ?? '')
	const activeServerUrl = $derived(activeServer ?? data.spec.servers.at(-1)?.url ?? '')
	const selected = $derived(data.operations.find((operation) => operation.id === selectedOperationId))
	const filtered = $derived(data.operations.filter((operation) => {
		const text = `${operation.method} ${operation.path} ${operation.operation.summary ?? ''} ${(operation.operation.tags ?? []).join(' ')}`.toLowerCase()
		return text.includes(query.trim().toLowerCase())
	}))
	const grouped = $derived(data.spec.tags.map((tag) => ({
		tag,
		operations: filtered.filter((operation) => operation.tags.includes(tag)),
	})).filter((group) => group.operations.length))
	const activeSchema = $derived(selectedSchema ? data.spec.schemas[selectedSchema] : undefined)
	const activeSchemaProperties = $derived(activeSchema?.properties as Record<string, Record<string, unknown>> | undefined)
	const selectedSecurity = $derived(selected?.operation.security as Record<string, string[]>[] | undefined)
	const activeServerLabel = $derived(data.spec.servers.find((server) => server.url === activeServerUrl)?.description ?? 'API server')

	function chooseOperation(id: string) {
		currentView = 'api'
		selectedSchema = ''
		selectedId = id
	}

	function chooseTag(tag: string) {
		const first = data.operations.find((operation) => operation.tags.includes(tag))
		if (first) chooseOperation(first.id)
	}

	function schemaType(schema: Record<string, unknown> | undefined): string {
		if (!schema) return '—'
		if (typeof schema.$ref === 'string') return schema.$ref.split('/').at(-1) ?? 'object'
		if (Array.isArray(schema.enum)) return schema.enum.map(String).join(' | ')
		if (Array.isArray(schema.oneOf)) return schema.oneOf.map((item) => schemaType(item as Record<string, unknown>)).join(' | ')
		const type = Array.isArray(schema.type) ? schema.type.join(' | ') : schema.type
		if (type === 'array' && schema.items) return `${schemaType(schema.items as Record<string, unknown>)}[]`
		return `${typeof type === 'string' ? type : 'any'}${typeof schema.format === 'string' ? ` (${schema.format})` : ''}`
	}

	function example(schema: Record<string, unknown> | undefined, depth = 0): unknown {
		if (!schema || depth > 5) return '…'
		if ('example' in schema) return schema.example
		if ('const' in schema) return schema.const
		if (Array.isArray(schema.enum)) return schema.enum[0]
		if (typeof schema.$ref === 'string') {
			const name = schema.$ref.split('/').at(-1) ?? ''
			return example(data.spec.schemas[name], depth + 1)
		}
		if (Array.isArray(schema.oneOf)) return example(schema.oneOf[0] as Record<string, unknown>, depth + 1)
		if (schema.type === 'array') return [example(schema.items as Record<string, unknown>, depth + 1)]
		if (schema.type === 'object' || schema.properties) {
			const properties = schema.properties as Record<string, Record<string, unknown>> | undefined
			if (!properties) return {}
			return Object.fromEntries(Object.entries(properties).map(([key, value]) => [key, example(value, depth + 1)]))
		}
		if (schema.type === 'integer' || schema.type === 'number') return 1
		if (schema.type === 'boolean') return true
		if (schema.format === 'date-time') return '2026-01-01T00:00:00Z'
		if (schema.format === 'int64') return 1
		return 'string'
	}

	function requestExample(operation: Record<string, unknown> | undefined): unknown {
		let requestBody = operation?.requestBody as Record<string, unknown> | undefined
		if (typeof requestBody?.$ref === 'string') {
			const name = requestBody.$ref.split('/').at(-1) ?? ''
			requestBody = data.spec.requestBodies[name]
		}
		const content = requestBody?.content as Record<string, Record<string, unknown>> | undefined
		const bodySchema = content?.['application/json']?.schema as Record<string, unknown> | undefined
		return example(bodySchema)
	}

	function responseList(operation: Record<string, unknown>): [string, Record<string, unknown>][] {
		const responses = operation.responses as Record<string, Record<string, unknown>> | undefined
		return Object.entries(responses ?? {}).map(([status, response]) => {
			if (typeof response.$ref !== 'string') return [status, response]
			const name = response.$ref.split('/').at(-1) ?? ''
			return [status, data.spec.responses[name] ?? response]
		})
	}

	async function copy(text: string) {
		await navigator.clipboard.writeText(text)
		copied = true
		setTimeout(() => copied = false, 1400)
	}

	function highlight(code: string, language: 'json' | 'bash') {
		return hljs.highlight(code, { language, ignoreIllegals: true }).value
	}

	function curlExample() {
		if (!selected) return ''
		const body = requestExample(selected.operation)
		const content = body && Object.keys(body as object).length ? ` -H 'Content-Type: application/json' --data '${JSON.stringify(body)}'` : ''
		return `curl -X ${selected.method} '${activeServerUrl}${selected.path}'${content}`
	}
</script>

<svelte:head>
	<title>{data.spec.info.title} · API Reference</title>
	<meta name="description" content={data.spec.info.description ?? 'Afterbell API reference.'} />
	<link rel="icon" href="/assets/afterbell-logo.png" />
</svelte:head>

<div class="docs-shell">
	<header class="topbar">
		<div class="brand-lockup">
			<img src="/assets/afterbell-logo.png" alt="Afterbell" />
			<span>Afterbell</span>
			<span class="slash">/</span>
			<span class="docs-label">API Reference</span>
		</div>
		<div class="header-actions">
			<a class="github-header" href="https://github.com/DavidNzube101/afterbell" target="_blank" rel="noreferrer">GitHub <ArrowUpRight size={14} /></a>
			<a class="source-link" href="/openapi.yaml" download>Download OpenAPI spec <ArrowDownToLine size={14} /></a>
		</div>
	</header>

	<div class="layout">
		<aside class="sidebar">
			<label class="search-box">
				<Search size={16} />
				<TextInput bind:value={query} placeholder="Search endpoints…" aria-label="Search endpoints" />
				{#if query}<button class="clear-search" aria-label="Clear search" onclick={() => query = ''}><X size={14} /></button>{/if}
			</label>
			<button class="intro-nav" class:active={currentView === 'introduction'} onclick={() => { currentView = 'introduction'; selectedSchema = '' }}>
				<span>Introduction</span><ArrowRight size={14} />
			</button>
			<div class="nav-title"><BookOpen size={14} /> Endpoints</div>
			<nav aria-label="API endpoints">
				{#each grouped as group}
					<div class="nav-group">
						<button class="group-heading" onclick={() => expandedTags[group.tag] = !expandedTags[group.tag]}>
							<span>{group.tag}</span><span class="group-end"><span class="count">{group.operations.length}</span>{#if expandedTags[group.tag]}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}</span>
						</button>
						{#if expandedTags[group.tag] !== false}
							{#each group.operations as operation}
								<button class="endpoint-link" class:active={selectedOperationId === operation.id} onclick={() => chooseOperation(operation.id)}>
									<span class="method-chip {operation.method.toLowerCase()}">{operation.method}</span>
									<span class="endpoint-path">{operation.path}</span>
								</button>
							{/each}
						{/if}
					</div>
				{/each}
				{#if filtered.length === 0}<p class="empty-nav">No endpoints match.</p>{/if}
			</nav>
			<div class="schema-nav">
				<div class="nav-title">Schemas</div>
				{#each Object.keys(data.spec.schemas) as schemaName}
					<button class="schema-link" class:active={selectedSchema === schemaName} onclick={() => { selectedSchema = schemaName; selectedId = '' }}><span>{schemaName}</span><ChevronRight size={13} /></button>
				{/each}
			</div>
			<div class="near-note"><img src="/assets/near-logo.jpeg" alt="NEAR" /><span>Powered by NEAR</span></div>
		</aside>

		<main class="content">
			<div class="content-toolbar">
				<div class="crumbs"><a href="/" onclick={(event) => { event.preventDefault(); currentView = 'introduction'; selectedSchema = '' }}>API</a><span>/</span><span>{currentView === 'introduction' ? 'Introduction' : selectedSchema || selected?.tags[0] || 'Reference'}</span></div>
				{#if currentView === 'api'}<div class="server-switch" aria-label="API server"><Tabs items={serverTabs} bind:value={() => activeServerUrl, (next) => activeServer = next} label="API server" size="small" /></div>{/if}
			</div>

			{#if currentView === 'introduction'}
				<section class="introduction">
					<h1>Afterbell</h1>
					<p class="intro-lead">An on-chain perpetual trading product for synthetic U.S. equity markets on NEAR.</p>
					<p class="intro-copy">A connected account brings trading and liquidity together. Otto answers questions using live market prices and account context.</p>
					<div class="product-grid">
						<Surface as="section" class="product-card" variant="soft" spacing="medium" radius="small">
							<Activity size={19} /><h2>Market data</h2><p>Market snapshots, historical prices, and trading volume.</p>
						</Surface>
						<Surface as="section" class="product-card" variant="soft" spacing="medium" radius="small">
							<Wallet size={19} /><h2>Accounts, trading, and liquidity</h2><p>Account setup, signable trade quotes, position actions, and liquidity operations.</p>
						</Surface>
						<Surface as="section" class="product-card" variant="soft" spacing="medium" radius="small">
							<Bot size={19} /><h2>Otto</h2><p>AI provider options, signed wallet sessions, and chat with live account context.</p>
						</Surface>
					</div>
					<div class="docs-intro">
						<div><h2>API reference</h2><p>This reference covers the HTTP API used by the Afterbell app and its services. It also includes health, notifications, and waitlist routes. Examples and schemas are generated from the OpenAPI specification.</p></div>
						<button class="browse-button" onclick={() => { currentView = 'api'; selectedSchema = ''; if (!selectedId) selectedId = data.operations[0]?.id }}>Browse endpoints <ArrowRight size={15} /></button>
					</div>
					<div class="tag-list" aria-label="API sections">
						{#each data.spec.tags as tag}<button onclick={() => chooseTag(tag)}>{tag}<ArrowRight size={13} /></button>{/each}
					</div>
				</section>
			{:else}
			<Surface as="section" class="overview" variant="soft" spacing="medium" radius="small">
				<div class="overview-title"><div><h1>{data.spec.info.title}</h1><p>{data.spec.info.description}</p></div><span class="version">v{data.spec.info.version}</span></div>
				<div class="server-line"><span class="server-dot"></span><code>{activeServerUrl}</code><span>{activeServerLabel}</span></div>
			</Surface>

			{#if selectedSchema && activeSchema}
				<section class="detail-section">
					<div class="section-title"><h2>{selectedSchema}</h2><button class="icon-action" aria-label="Copy schema" onclick={() => void copy(JSON.stringify(activeSchema, null, 2))}>{#if copied}<Check size={15} />{:else}<Copy size={15} />{/if}</button></div>
					<p class="section-description">{String(activeSchema.description ?? schemaType(activeSchema))}</p>
					{#if activeSchemaProperties}
						<div class="table-wrap schema-properties"><table><thead><tr><th>Property</th><th>Type</th><th>Description</th></tr></thead><tbody>
							{#each Object.entries(activeSchemaProperties) as [property, propertySchema]}
								<tr><td><code>{property}</code>{#if Array.isArray(activeSchema.required) && activeSchema.required.includes(property)}<span class="required">required</span>{/if}</td><td>{schemaType(propertySchema)}</td><td>{String(propertySchema.description ?? '')}</td></tr>
							{/each}
						</tbody></table></div>
					{/if}
					<pre class="code-block"><code>{@html highlight(JSON.stringify(activeSchema, null, 2), 'json')}</code></pre>
				</section>
			{:else if selected}
				<section class="detail-section">
					<div class="operation-heading">
						<span class="method-chip large {selected.method.toLowerCase()}">{selected.method}</span>
						<code class="operation-path">{selected.path}</code>
						<button class="icon-action" aria-label="Copy endpoint path" onclick={() => void copy(`${activeServerUrl}${selected.path}`)}>{#if copied}<Check size={15} />{:else}<Copy size={15} />{/if}</button>
					</div>
					<h2>{String(selected.operation.summary ?? selected.path)}</h2>
					{#if selected.operation.description}<p class="section-description">{String(selected.operation.description)}</p>{/if}
					{#if selectedSecurity?.length}
						<div class="security-note"><span>Authentication</span>{#each selectedSecurity.flatMap((item) => Object.keys(item)) as scheme}<code>{scheme}</code>{/each}</div>
					{/if}
					{#if Array.isArray(selected.operation.parameters) && selected.operation.parameters.length}
						<h3>Parameters</h3>
						<div class="table-wrap"><table><thead><tr><th>Name</th><th>In</th><th>Type</th><th>Description</th></tr></thead><tbody>
							{#each selected.operation.parameters as rawParameter}
								{@const parameter = rawParameter as Record<string, unknown>}
								<tr><td><code>{String(parameter.name)}</code>{#if parameter.required}<span class="required">required</span>{/if}</td><td>{String(parameter.in)}</td><td>{schemaType(parameter.schema as Record<string, unknown>)}</td><td>{String(parameter.description ?? '')}</td></tr>
							{/each}
						</tbody></table></div>
					{/if}
					{#if selected.operation.requestBody}
						<h3>Request body <span class="content-type">application/json</span></h3>
						<p class="field-note">{String((selected.operation.requestBody as Record<string, unknown>).description ?? '')}</p>
						<pre class="code-block"><code>{@html highlight(JSON.stringify(requestExample(selected.operation), null, 2), 'json')}</code></pre>
					{/if}
					<h3>Responses</h3>
					<div class="responses">
						{#each responseList(selected.operation) as [statusCode, response]}
							{@const content = response.content as Record<string, Record<string, unknown>> | undefined}
							{@const responseSchema = content?.['application/json']?.schema as Record<string, unknown> | undefined}
							<Surface as="section" class="response-card" variant="soft" spacing="small" radius="small">
								<div class="response-head"><span class="response-code" class:ok={statusCode.startsWith('2')}>{statusCode}</span><span>{String(response.description ?? '')}</span></div>
								{#if responseSchema}<p class="response-type">{schemaType(responseSchema)}{#if content?.['application/json']}<span>application/json</span>{/if}</p><pre class="code-block compact"><code>{@html highlight(JSON.stringify(example(responseSchema), null, 2), 'json')}</code></pre>{/if}
							</Surface>
						{/each}
					</div>
					<h3>Example request</h3>
					<div class="code-heading"><span>cURL</span><button class="icon-action" aria-label="Copy cURL example" onclick={() => void copy(curlExample())}>{#if copied}<Check size={15} />{:else}<Copy size={15} />{/if}</button></div>
					<pre class="code-block"><code>{@html highlight(curlExample(), 'bash')}</code></pre>
				</section>
			{:else}
				<div class="no-selection">Select an endpoint or schema.</div>
			{/if}
			{/if}

			<footer class="site-footer">
				<nav class="footer-links" aria-label="Afterbell links">
					<a href="https://x.com/afterbellfx" target="_blank" rel="noreferrer">X</a>
					<a href="https://afterbellfx.xyz" target="_blank" rel="noreferrer">Website</a>
					<a href="https://github.com/DavidNzube101/afterbell" target="_blank" rel="noreferrer">GitHub</a>
					<a class="star-link" href="https://github.com/DavidNzube101/afterbell" target="_blank" rel="noreferrer"><Star size={13} /> Star on GitHub</a>
				</nav>
			</footer>
		</main>
	</div>
</div>
