
$env:AC_DATABASE_URL="postgresql://postgres:dummy@database-1.cluster-ce5gog82gxo1.us-east-1.rds.amazonaws.com:5432/aishelter?sslmode=require"
$url = aws rds generate-db-auth-token --hostname database-1.cluster-ce5gog82gxo1.us-east-1.rds.amazonaws.com --port 5432 --region us-east-1 --username postgres
psql "postgresql://postgres:$url@database-1.cluster-ce5gog82gxo1.us-east-1.rds.amazonaws.com:5432/aishelter?sslmode=require"

