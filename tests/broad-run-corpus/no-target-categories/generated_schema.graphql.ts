/* eslint-disable */
// @generated
// This file was automatically generated and should not be edited.
// NoTargetReason::GeneratedSchema — synthetic GraphQL/codegen output.

export type Scalars = {
  ID: string;
  String: string;
  Boolean: boolean;
  Int: number;
};

export type User = {
  __typename?: "User";
  id: Scalars["ID"];
  email: Scalars["String"];
};

export type Query = {
  __typename?: "Query";
  user?: User | null;
};
