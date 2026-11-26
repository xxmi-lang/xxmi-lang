import { defineRailway, github, preserve, project, service, volume } from 'railway/iac';

export default defineRailway(() => {
  const xxmiMcpVolume = volume('xxmi-mcp-volume', {
    alerts: { usage: { '100': {}, '80': {}, '95': {} } },
    allowOnlineResize: true,
    region: 'europe-west4-drams3a',
    sizeMB: 50000,
  });
  const xxmiMcp = service('xxmi-mcp', {
    source: github('xxmi-lang/xxmi-lang', { checkSuites: true }),
    build: {
      buildEnvironment: 'V3',
      builder: 'DOCKERFILE',
      dockerfilePath: '/docker/mcp/Dockerfile',
    },
    deploy: { restartPolicyType: 'ON_FAILURE' },
    healthcheck: '/healthz',
    healthcheckTimeout: 60,
    replicas: { 'europe-west4-drams3a': 1 },
    volumeMounts: { '/data': xxmiMcpVolume },
    env: {
      ADMIN_TOKEN: preserve(),
      RAILWAY_DOCKERFILE_PATH: preserve(),
      TRUSTED_PROXY_HOPS: preserve(),
    },
  });

  return project('xxmi-lang', {
    resources: [xxmiMcp, xxmiMcpVolume],
  });
});
