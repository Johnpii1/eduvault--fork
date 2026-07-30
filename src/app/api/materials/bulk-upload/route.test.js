import { describe, it, expect, vi } from 'vitest'

vi.mock('@/lib/pinata', () => ({
  pinata: {
    upload: {
      public: {
        file: vi.fn().mockImplementation(async (file) => ({
          cid: `QmMockCid_${file.name}`,
        })),
      },
    },
    gateways: {
      public: {
        convert: vi.fn().mockImplementation(async (cid) => `https://gateway.pinata.cloud/ipfs/${cid}`),
      },
    },
  },
}))

vi.mock('@/lib/ipfs/uploadValidator', () => ({
  validateUploadedFile: vi.fn().mockResolvedValue({ valid: true }),
}))

vi.mock('@/lib/api/audit', () => ({
  auditLog: vi.fn(),
}))

vi.mock('@/lib/api/hardening', () => ({
  withApiHardening: vi.fn((req, options, handler) => handler()),
}))

import { POST } from './route'

describe('POST /api/materials/bulk-upload', () => {
  it('returns 400 when no files are provided', async () => {
    const formData = new FormData()
    const req = {
      formData: async () => formData,
      headers: new Headers(),
    }

    const res = await POST(req)
    const json = await res.json()

    expect(res.status).toBe(400)
    expect(json.error).toMatch(/No files provided/)
  })

  it('returns 400 when batch size exceeds 10 files', async () => {
    const formData = new FormData()
    for (let i = 0; i < 11; i++) {
      const file = new File(['content'], `test_${i}.pdf`, { type: 'application/pdf' })
      formData.append('files', file)
    }

    const req = {
      formData: async () => formData,
      headers: new Headers(),
    }

    const res = await POST(req)
    const json = await res.json()

    expect(res.status).toBe(400)
    expect(json.error).toMatch(/Maximum allowed files per batch is 10/)
  })

  it('processes batch files concurrently and returns CIDs', async () => {
    const formData = new FormData()
    const file1 = new File(['content1'], 'test1.pdf', { type: 'application/pdf' })
    const file2 = new File(['content2'], 'test2.txt', { type: 'text/plain' })
    formData.append('files', file1)
    formData.append('files', file2)

    const req = {
      formData: async () => formData,
      headers: new Headers(),
    }

    const res = await POST(req)
    const json = await res.json()

    expect(res.status).toBe(200)
    expect(json.success).toBe(true)
    expect(json.total).toBe(2)
    expect(json.uploadedCount).toBe(2)
    expect(json.files.length).toBe(2)
    expect(json.files[0].cid).toBe('QmMockCid_test1.pdf')
    expect(json.files[1].cid).toBe('QmMockCid_test2.txt')
  })
})
