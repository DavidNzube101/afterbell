import source from '../../../../openapi.yaml?raw'
import type { RequestHandler } from './$types'

export const GET: RequestHandler = () => new Response(source, {
	headers: {
		'Content-Type': 'application/yaml; charset=utf-8',
		'Content-Disposition': 'inline; filename="openapi.yaml"',
		'Cache-Control': 'public, max-age=300',
	},
})
