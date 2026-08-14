{{- define "open-foundry.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "open-foundry.fullname" -}}
{{- printf "%s-%s" (include "open-foundry.name" .root) .serviceName | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "open-foundry.mergedService" -}}
{{- mergeOverwrite (deepCopy .root.Values.serviceDefaults) .service | toYaml -}}
{{- end -}}

{{- define "open-foundry.upstreamEnv" -}}
{{- printf "%s_URL" (upper (replace "-" "_" .)) -}}
{{- end -}}

{{- define "open-foundry.upstreamUrl" -}}
{{- $scheme := "http" -}}
{{- printf "%s://%s:%v" $scheme (include "open-foundry.fullname" .) .port -}}
{{- end -}}

{{- define "open-foundry.jwtSecretName" -}}
{{- if .Values.global.jwt.existingSecret -}}
{{- .Values.global.jwt.existingSecret -}}
{{- else -}}
{{- printf "%s-jwt" (include "open-foundry.name" .) -}}
{{- end -}}
{{- end -}}

{{- define "open-foundry.dbSecretName" -}}
{{- if .Values.global.database.existingSecret -}}
{{- .Values.global.database.existingSecret -}}
{{- else -}}
{{- printf "%s-db" (include "open-foundry.name" .) -}}
{{- end -}}
{{- end -}}

{{- define "open-foundry.databaseUrlKey" -}}
{{- printf "%s-database-url" . -}}
{{- end -}}

{{- define "open-foundry.migrationUrlKey" -}}
{{- printf "%s-migration-url" . -}}
{{- end -}}

{{- define "open-foundry.runtimeDatabaseUrl" -}}
{{- $db := .root.Values.global.database -}}
{{- printf "postgres://%s:%s@%s:%v/%s" $db.runtimeUser $db.runtimePassword $db.host $db.port .database -}}
{{- end -}}

{{- define "open-foundry.migrationDatabaseUrl" -}}
{{- $db := .root.Values.global.database -}}
{{- printf "postgres://%s:%s@%s:%v/%s" $db.migrationUser $db.migrationPassword $db.host $db.port .database -}}
{{- end -}}
