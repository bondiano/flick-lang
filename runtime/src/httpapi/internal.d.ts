// The Swagger UI and Scalar bundles inside the pinned Effect, which serves
// them inline; their own declarations export nothing.
declare module "effect/http-api/internal/httpApiSwagger" {
  export const css: string;
  export const javascript: string;
}
declare module "effect/http-api/internal/httpApiScalar" {
  export const javascript: string;
}
