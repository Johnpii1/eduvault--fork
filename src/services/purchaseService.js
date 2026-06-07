import { apiClient } from '@/lib/api/apiClient';

export const purchaseService = {
  createPurchase: async (purchaseData) => {
    return apiClient('/api/purchase', { body: purchaseData });
  },

  checkEntitlement: async (materialId, buyerAddress) => {
    const params = new URLSearchParams({ materialId, buyerAddress });
    return apiClient(`/api/entitlements?${params.toString()}`);
  },

  checkBatchEntitlements: async (materialIds) => {
    return apiClient('/api/entitlements/batch', { body: { materialIds } });
  },

  getPurchaseHistory: async () => {
    return apiClient('/api/purchase');
  },
};

