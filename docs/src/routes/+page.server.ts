import { parse } from 'yaml'
import source from '../../../openapi.yaml?raw'
import type { PageServerLoad } from './$types'

type ApiSpec = {
	openapi: string
	info: { title: string; version: string; description?: string }
	servers?: { url: string; description?: string }[]
	tags?: { name: string; description?: string }[]
	paths: Record<string, Record<string, ApiOperation>>
	components?: {
		schemas?: Record<string, Record<string, unknown>>
		responses?: Record<string, Record<string, unknown>>
		requestBodies?: Record<string, Record<string, unknown>>
		securitySchemes?: Record<string, Record<string, unknown>>
	}
}

type ApiOperation = {
	tags?: string[]
	summary?: string
	description?: string
	parameters?: Record<string, unknown>[]
	requestBody?: Record<string, unknown>
	responses?: Record<string, Record<string, unknown>>
	[key: string]: unknown
}

const spec = parse(source) as ApiSpec
const methods = new Set(['get', 'post', 'put', 'patch', 'delete', 'options', 'head'])

export const load: PageServerLoad = () => {
	const operations = Object.entries(spec.paths).flatMap(([path, pathItem]) =>
		Object.entries(pathItem)
			.filter(([method]) => methods.has(method.toLowerCase()))
			.map(([method, operation]) => ({
				id: `${method.toUpperCase()} ${path}`,
				method: method.toUpperCase(),
				path,
				tags: operation.tags ?? ['Other'],
				operation,
			})),
	)
	const tags = spec.tags?.map((tag) => tag.name) ?? [...new Set(operations.flatMap((operation) => operation.tags))]
	return {
		spec: {
			openapi: spec.openapi,
			info: spec.info,
			servers: spec.servers ?? [],
			tags,
			schemas: spec.components?.schemas ?? {},
			responses: spec.components?.responses ?? {},
			requestBodies: spec.components?.requestBodies ?? {},
			securitySchemes: spec.components?.securitySchemes ?? {},
		},
		operations,
	}
}
