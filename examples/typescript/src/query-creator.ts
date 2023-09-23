import { Pool } from 'pg';

import { ComparisonOperatorExpression, ExpressionBuilder, ExpressionWrapper, Kysely, PostgresDialect, SqlBool, sql } from 'kysely';
import { isEmpty } from 'lodash';

type FacetValidationError = {
    message: string
    facet?: string
    component?: string
}

const logger = {
    debug: (s: string) => { },
    info: (s: string) => { },
    warn: (s: string) => { },
    error: (s: string) => { },
}

type Maybe<T> = T | null;
type InputMaybe<T> = Maybe<T>;
type Scalars = {
    ID: string;
    String: string;
    Boolean: boolean;
    Int: number;
    Float: number;
    BigInt: number;
    Date: Date | string;
    DateTime: Date | string;
    Time: Date | string;
};

type BinaryFieldInputValue = {
    present: Scalars['Boolean'];
};

type BinaryInputField = {
    __typename?: 'BinaryInputField';
    _p?: Maybe<Scalars['Int']>;
};

type Bound = {
    __typename?: 'Bound';
    defaultValue?: Maybe<Scalars['Float']>;
    max?: Maybe<Scalars['Float']>;
    min?: Maybe<Scalars['Float']>;
    required?: Maybe<Scalars['Boolean']>;
};

type ChoiceFieldInputValue = {
    name?: InputMaybe<Scalars['String']>;
    values: Array<Scalars['String']>;
};

type ChoiceInputField = {
    __typename?: 'ChoiceInputField';
    choices: Array<LabeledValue>;
    maxSelections?: Maybe<Scalars['Int']>;
    minSelections?: Maybe<Scalars['Int']>;
};

type Coordinates = {
    latitude: Scalars['Float'];
    longitude: Scalars['Float'];
};

type Facet = {
    __typename?: 'Facet';
    components: Array<FacetComponent>;
    label: Scalars['String'];
    maximumOccurrences: Scalars['Int'];
    minimumOccurrences: Scalars['Int'];
    name: Scalars['String'];
    negatable?: Maybe<Scalars['Boolean']>;
    relatesToFields?: Maybe<Array<Scalars['String']>>;
    shortLabel?: Maybe<Scalars['String']>;
    theme: Theme;
};

type FacetComponent = {
    __typename?: 'FacetComponent';
    fields: Array<InputFieldUnion>;
    label?: Maybe<Scalars['String']>;
    name: Scalars['String'];
    required: Scalars['Boolean'];
    requiredInputs?: Maybe<Array<PersonalizationInput>>;
};

type FacetComponentInputValues = {
    fields: Array<FieldInputValue>;
};

type FacetInputValue = {
    inputs: Array<FacetComponentInputValues>;
    name: Scalars['String'];
    negate?: InputMaybe<Scalars['Boolean']>;
};

type FieldInputValue = {
    binary?: InputMaybe<BinaryFieldInputValue>;
    choice?: InputMaybe<ChoiceFieldInputValue>;
    number?: InputMaybe<NumberFieldInputValue>;
    range?: InputMaybe<RangeFieldInputValue>;
    text?: InputMaybe<TextFieldInputValue>;
};

type FieldValueType =
    | 'BINARY'
    | 'DISTANCE'
    | 'FLOAT'
    | 'INTEGER'
    | 'MONEY'
    | 'OBJECT'
    | 'PERCENTAGE'
    | 'STRING'
    | 'URL';

type InputFieldUnion = {
    __typename?: 'InputFieldUnion';
    binary?: Maybe<BinaryInputField>;
    choice?: Maybe<ChoiceInputField>;
    label?: Maybe<Scalars['String']>;
    number?: Maybe<NumberInputField>;
    range?: Maybe<RangeInputField>;
    text?: Maybe<TextInputField>;
};

type LabeledValue = {
    __typename?: 'LabeledValue';
    label: Scalars['String'];
    value: Scalars['String'];
};

type LocationInput = {
    coordinates?: InputMaybe<Coordinates>;
    zipCode?: InputMaybe<Scalars['String']>;
};

type LocationOutput = {
    __typename?: 'LocationOutput';
    city?: Maybe<Scalars['String']>;
    country?: Maybe<Scalars['String']>;
    latitude?: Maybe<Scalars['Float']>;
    longitude?: Maybe<Scalars['Float']>;
    state?: Maybe<Scalars['String']>;
    stateCode?: Maybe<Scalars['String']>;
    zipCode?: Maybe<Scalars['String']>;
};

type NumberFieldInputValue = {
    name?: InputMaybe<Scalars['String']>;
    value?: InputMaybe<Scalars['Float']>;
};

type NumberInputField = {
    __typename?: 'NumberInputField';
    defaultValue?: Maybe<Scalars['Float']>;
    max?: Maybe<Scalars['Float']>;
    min?: Maybe<Scalars['Float']>;
    step?: Maybe<Scalars['Float']>;
    unit?: Maybe<NumberUnit>;
};

type NumberUnit =
    | 'DOLLARS'
    | 'HOURS'
    | 'MILES'
    | 'PERCENTAGE';

type NumericColumnRange = {
    __typename?: 'NumericColumnRange';
    max?: Maybe<Scalars['Float']>;
    min?: Maybe<Scalars['Float']>;
};

type OutputColumn = {
    __typename?: 'OutputColumn';
    category?: Maybe<Scalars['String']>;
    label: Scalars['String'];
    name: Scalars['String'];
    range?: Maybe<NumericColumnRange>;
    required?: Maybe<Scalars['Boolean']>;
    requiredInputs?: Maybe<Array<PersonalizationInput>>;
    shortLabel: Scalars['String'];
    sortable?: Maybe<Scalars['Boolean']>;
    theme: Theme;
    unit?: Maybe<Unit>;
    valueStructure?: Maybe<ValueStructure>;
    valueType?: Maybe<FieldValueType>;
    weighable?: Maybe<Scalars['Boolean']>;
};

type PersonalizationInput =
    | 'LOCATION';

type RangeFieldInputValue = {
    high?: InputMaybe<Scalars['Float']>;
    low?: InputMaybe<Scalars['Float']>;
    name?: InputMaybe<Scalars['String']>;
};

type RangeInputField = {
    __typename?: 'RangeInputField';
    highBound?: Maybe<Bound>;
    lowBound?: Maybe<Bound>;
    maxDelta?: Maybe<Scalars['Float']>;
    minDelta?: Maybe<Scalars['Float']>;
    step?: Maybe<Scalars['Float']>;
    unit?: Maybe<NumberUnit>;
};

type SearchInput = {
    columnNames?: InputMaybe<Array<Scalars['String']>>;
    facetInputs?: InputMaybe<Array<FacetInputValue>>;
    location?: InputMaybe<LocationInput>;
    page?: InputMaybe<Scalars['Int']>;
    quickSearch?: InputMaybe<Scalars['String']>;
    rankingsToBlend?: InputMaybe<Array<Scalars['String']>>;
    sort?: InputMaybe<Array<Sort>>;
    unitIds?: InputMaybe<Array<Scalars['String']>>;
    weights?: InputMaybe<Array<WeightsInput>>;
};

type Sort = {
    direction: SortDirection;
    field: Scalars['String'];
};

type SortDirection =
    | 'ASC'
    | 'DESC';

type TextFieldInputValue = {
    name?: InputMaybe<Scalars['String']>;
    value?: InputMaybe<Scalars['String']>;
};

type TextInputField = {
    __typename?: 'TextInputField';
    max?: Maybe<Scalars['Int']>;
    min?: Maybe<Scalars['Int']>;
    pattern?: Maybe<Scalars['String']>;
};

type Unit =
    | 'MILES';

type ValueStructure =
    | 'MAP'
    | 'OBJECT'
    | 'SCALAR'
    | 'VECTOR';

type WeightingMethod =
    | 'BIGGER_IS_BETTER'
    | 'IDEAL_TARGET'
    | 'SMALLER_IS_BETTER';

type WeightsInput = {
    field: Scalars['String'];
    target?: InputMaybe<Scalars['Float']>;
    weight: Scalars['Float'];
    weightingMethod: WeightingMethod;
};

const Theme = {
    ACADEMICS: 'ACADEMICS',
    ADMISSIONS: 'ADMISSIONS',
    ATHLETICS: 'ATHLETICS',
    CORE: 'CORE',
    COST: 'COST',
    CULTURAL_ENVIRONMENT: 'CULTURAL_ENVIRONMENT',
    DEMOGRAPHICS: 'DEMOGRAPHICS',
    FIELDS_OF_STUDY: 'FIELDS_OF_STUDY',
    LOCATION: 'LOCATION',
    METADATA: 'METADATA',
    OUTCOMES: 'OUTCOMES',
    PHYSICAL_ENVIRONMENT: 'PHYSICAL_ENVIRONMENT',
    REPUTATION: 'REPUTATION',
    STUDENT_LIFE: 'STUDENT_LIFE',
    WEATHER: 'WEATHER',
    OTHER: 'OTHER',
} as const
type Theme = (typeof Theme)[keyof typeof Theme]

const Datakeys = ['applications', 'ssocs', 'ncaa', 'nces_fos', 'nces', 'koppen', 'partisan_lean_538', 'metro_area', 'aau_list', 'nces_subjects'] as const
type Datakey = typeof Datakeys[number]

const RANGED_TYPES = ['DISTANCE', 'INTEGER', 'MONEY', 'FLOAT', 'PERCENTAGE'] as const satisfies readonly FieldValueType[]
type RangedType = typeof RANGED_TYPES[number]

const NUMERIC_TYPES = [...RANGED_TYPES, 'BINARY', 'PERCENTAGE'] as const satisfies readonly FieldValueType[]
type NumericValueType = typeof NUMERIC_TYPES[number]

const escapeAndQuote = (value: string) => `'${value.replaceAll(/[']/g, "''")}'`
const escapeAndQuoteAll = (values: string[]) => values.map(escapeAndQuote)

type BaseResultField = Pick<OutputColumn, 'name' | 'theme' | 'category' | 'sortable' | 'required' | 'label' | 'valueStructure' | 'requiredInputs'> & {
    type: string
    priority?: number
    shortLabel?: string
    unit?: Unit
    valueLabels?: readonly LabeledValue[]
    postProcessor?: (value: any) => any
} & ({
    //  TODO: distinguish between storage type and output/display type
    //  how much is this reinventing JSON Schema?
    valueType: Exclude<FieldValueType, RangedType>
} | {
    valueType: RangedType
    range: {
        min: number
        max: number
    }
})

type ColumnResultField = {
    type: 'column'
} & BaseResultField

const JsonColumns = ['nces', 'weather', 'athletics', 'fields_of_study', 'admissions', 'misc'] as const
type JsonColumn = typeof JsonColumns[number]
type JsonResultField<JC extends JsonColumn> = {
    type: 'json'
    jsonColumn: JC
    datakey: Datakey
    path: readonly string[]
} & BaseResultField

type ExpressionField = {
    "type": "expression",
} & ({
    "operation": 'SUM' | 'FORMAT' | 'COALESCE',
    "inputs": string[],
} | {
    "operation": 'COMPARE',
    "comparisons": {
        "input": string,
        "operator": ComparisonOperatorExpression | '?|' | '?&'
        "test": string | number | boolean
        "result": string | number | boolean
    }[]
    "default"?: string | number | boolean
})
    & BaseResultField

type Formula = (inputs: SearchPersonalization) => string | undefined
type PersonalizedField = {
    type: 'personalized',
    formula: Formula,
} & BaseResultField

type SecondaryQueryField = {
    "type": "secondary",
} & BaseResultField

type ResultField = ColumnResultField | JsonResultField<any> | PersonalizedField | ExpressionField | SecondaryQueryField

const rfPath = (rf: JsonResultField<JsonColumn>) => {
    const datasetColumn = `data->'${rf.datakey}'`

    if (!rf.path || rf.path.length == 0) {
        return datasetColumn
    }

    const pathEscaped = escapeAndQuoteAll([...rf.path])
    if (rf.valueStructure == 'SCALAR') {
        const corePath = "(" + [datasetColumn, ...pathEscaped.slice(0, pathEscaped.length - 1)].join(' -> ') + '->>' + pathEscaped[pathEscaped.length - 1] + ")"
        if (NUMERIC_TYPES.includes(rf.valueType as NumericValueType)) {
            return corePath + "::numeric"
        }
        return corePath
    }
    return "(" + [datasetColumn, ...pathEscaped].join(' -> ') + ")"
}

const resolveReference = (f: ResultField, allResultFields: Record<string, ResultField>, personalization: SearchPersonalization): string | undefined => {
    if (f.type == 'column') {
        return f.name
    }

    if (f.type == 'json') {
        return rfPath(f)
    }

    if (f.type == 'expression') {

        const toLiteral = (value: string | number | boolean | null): string => {
            if (value == null) {
                return 'NULL'
            }
            if (typeof value == 'string') {
                return escapeAndQuote(value)
            }
            return value.toString()
        }

        if (f.operation == 'COMPARE') {
            const cases: string[] = []
            for (const comparison of f.comparisons) {
                const rf = allResultFields[comparison.input]
                if (rf) {
                    const resolved = resolveReference(rf, allResultFields, personalization)
                    if (resolved) {
                        const clause = `WHEN ${resolved} ${comparison.operator} ${toLiteral(comparison.test)}
                                THEN ${comparison.result}`
                        cases.push(clause)
                    }
                }
            }

            if (cases.length == 0) {
                if (f.default) {
                    return toLiteral(f.default)
                }
                return undefined
            }

            const expression = `CASE ${cases.join('\n')} ELSE ${toLiteral(f.default ?? null)} END`
            return expression
        }

        const references: string[] = []
        for (const i of f.inputs) {
            const rf = allResultFields[i]
            if (rf) {
                const resolved = resolveReference(rf, allResultFields, personalization)
                if (resolved) {
                    references.push(resolved)
                }
            }
        }

        if (references.length < f.inputs.length) {
            logger.warn(`could not resolve references ${JSON.stringify(f.inputs)} for expression ${f.name}`)
            return undefined
        }

        if (f.operation == 'SUM') {
            const expression = references.join(' + ')
            return expression
        }
        if (f.operation == 'FORMAT') {
            const formatString = references.map(r => '%s').join(', ')
            const ingredients = references.join(',')
            const expression = `FORMAT('${formatString}', ${ingredients})`
            return expression
        }
        if (f.operation == 'COALESCE') {
            const expression = `COALESCE(${references.join(', ')})`
            return expression
        }

        throw new Error(`unknown expression operation ${f.operation}`)
    }

    if (f.type == 'personalized') {
        const providedInputs: PersonalizationInput[] = []
        if (personalization.location) {
            providedInputs.push('LOCATION')
        }

        const requiredInputs = f.requiredInputs ?? []
        const hasAllRequired = requiredInputs.every(input => providedInputs.includes(input))
        if (!hasAllRequired) {
            return undefined
        }

        const result = f.formula.call(f, personalization)
        return result
    }

    throw new Error(`unknown result field type ${(f as any).type}`)
}

type SearchPersonalization = {
    location?: LocationOutput
}

// const enforcer = await newEnforcer(model, policy);

const isWeighable = (rf: ResultField) => {
    const weighable = RANGED_TYPES.includes(rf.valueType as RangedType) && rf.sortable && isEmpty(rf.valueLabels)
    return weighable
}

const constructWeightExpression = (availableResultFieldCombined: Record<string, ResultField>,
    weights: WeightsInput[], personalization: SearchPersonalization): [string, Record<string, string>] => {
    const expressions: Record<string, string> = {}

    weights.forEach(w => {
        const rf = availableResultFieldCombined[w.field]
        if (!rf) {
            logger.warn(`unsatisfied weight field '${w.field}'; ignoring`)
            return
        }

        if (isWeighable(rf)) {
            if ('range' in rf && rf.range) {
                const min = rf.range?.min ?? 0
                const max = rf.range?.max ?? 1
                //  ABS in case of a mistake with setting the hard min/max
                const rawScore = `ABS(${w.weight} * ("${rf.name}" - ${min}) / (${max} - ${min}))`
                const scoringExpression = w.weightingMethod == 'SMALLER_IS_BETTER'
                    ? `1 - ${rawScore}`
                    : rawScore

                const nullHandlingExpression = `
          CASE
            WHEN "${rf.name}" IS NULL
              THEN 0
            ELSE ${scoringExpression}
          END
        `

                expressions[w.field] = nullHandlingExpression
            }
        } else {
            logger.warn(`unsatisfied weight field '${w.field}'; ignoring`)
        }
    })

    const sumSql = Object.values(expressions).join(' + ')
    return [sumSql, expressions]
}

type Facetation = { status: 'invalid', error: FacetValidationError[] } | { status: 'valid', clause: Clause }

interface BasicClause {
    //  TODO: include the path here
    //  TODO: maybe the resultField also/instead
}
interface JsonContainsClause extends BasicClause {
    type: 'json-contains'
    matchObject: any
}

interface SimpleClause extends BasicClause {
    type: 'simple'
    left: string
    operator: ComparisonOperatorExpression | '?|' | '?&'  //  TODO: '@?'
    right: any
}

interface NegativeClause extends BasicClause {
    type: 'negative'
    clause: Clause
}

interface ComplexClause extends BasicClause {
    type: 'complex'
    operator: 'AND' | 'OR'
    clauses: Clause[]
}

type Clause = (SimpleClause | ComplexClause | NegativeClause | JsonContainsClause)

interface FacetBuilder {
    getName(): string
    get(): ExtendedFacet
    apply(availableResultFields: Record<string, ResultField>, personalization: SearchPersonalization, facetInput: FacetInputValue): Facetation
}

type ExtendedFacet = Omit<Facet, 'components' | 'eligibleCustomers'> & {
    components: ExtendedFacetComponent[]
    semantics: 'OR' | 'AND'
}

//  includes resultFields, which is not sent to the client
type ExtendedFacetComponent = FacetComponent & {
    resultFields: BaseResultField[]
}

import type { ColumnType } from 'kysely';
type Generated<T> = T extends ColumnType<infer S, infer I, infer U>
    ? ColumnType<S, I | undefined, U>
    : ColumnType<T, T | undefined, T>
type Timestamp = ColumnType<Date, Date | string, Date | string>

type AnnualDataset = {
    id: string
    unit_id: string
    created: Generated<Timestamp>
    updated: Generated<Timestamp>
    version: Generated<number>
    theme: Theme
    datakey: string
    is_latest: Generated<boolean>
    year: number
    data: unknown
}

type CIPData = {
    id: string
    code: string
    description: string
}

type Conference = {
    name: string
}

type Dataset = {
    id: string
    unit_id: string
    created: Generated<Timestamp>
    updated: Generated<Timestamp>
    version: Generated<number>
    theme: Theme
    datakey: string
    data: unknown
}

type Institution = {
    unit_id: string
    ope6_id: string
    ope8_id: string
    created: Generated<Timestamp>
    updated: Generated<Timestamp>
    version: Generated<number>
    institution_name: string
    city: string
    state: string
    nces: unknown | null
    weather: unknown | null
    rankings: unknown | null
    athletics: unknown | null
    admissions: unknown | null
    fields_of_study: unknown | null
    misc: unknown | null
}

type State = {
    code: string
    name: string
}

type ZipCoordinates = {
    zip_code: string
    additional: unknown
}

type DB = {
    annual_dataset: AnnualDataset
    cip_data: CIPData
    conference: Conference
    dataset: Dataset
    institution: Institution
    state: State
    zip_coordinates: ZipCoordinates
}

interface AnnualDatasetMerged {
    unit_id: string
    theme: Theme
    datakey: string
    maxyear: number
    data: any
}

interface AnnualDatasetLatest {
    unit_id: string
    theme: Theme
    datakey: string
    year: number
    data: unknown
}

//  TODO: turn Pick into Omit
interface DatasetMerged extends Omit<Institution, 'created' | 'updated' | 'version' | 'nces' | 'weather' | 'rankings' | 'athletics' | 'admissions' | 'fields_of_study' | 'misc'> {
    unit_id: string
    theme: Theme
    datakey: string
    maxyear?: number
    name_search_vector: string
    name_location_search_vector: string
    data: any
}

interface DatasetLatest {
    unit_id: string
    theme: Theme
    datakey: string
    year?: number
    data: unknown
}

interface DBGenPlusUnsupported extends DB {
    institution: DB['institution'] & {
        coordinates: string
    },
    annual_dataset_merged: AnnualDatasetMerged,
    annual_dataset_latest: AnnualDatasetLatest,
    dataset_merged: DatasetMerged,
    dataset_latest: DatasetLatest,
}

const SCHEMAS: Record<keyof DBGenPlusUnsupported, string> = {
    zip_coordinates: 'location',
    state: 'location',

    cip_data: 'search_base',
    conference: 'search_base',
    institution: 'search_base',
    annual_dataset: 'search_base',
    dataset: 'search_base',

    annual_dataset_latest: 'search_base', //  TODO: delete
    annual_dataset_merged: 'search_base', //  TODO: delete
    dataset_latest: 'search_base',
    dataset_merged: 'search_base',
} as const

//    [K in keyof T as K extends string ? `${P}${K}` : never]: T[K]

type DBWithSchemas = {
    [K in keyof typeof SCHEMAS as `${typeof SCHEMAS[K]}.${K}`]: DBGenPlusUnsupported[K]
}

const db = new Kysely<DBWithSchemas>({
    dialect: new PostgresDialect({
        pool: new Pool({
            connectionString: process.env['DATABASE_URL']
        })
    })
})

const constructUnfacetedSearchQuery = ({
    availableResultFieldCombined,
    requestedFields,
    personalization,
    unitIds,
}: {
    availableResultFieldCombined: Record<string, ResultField>
    requestedFields: ResultField[]
    personalization: SearchPersonalization
    unitIds?: string[]
}) => {
    const fields = requestedFields.filter(f => f.type != 'secondary')

    const carnegieUndergradRequested = fields.find(f => f.name == 'carnegie_undergrad')
    if (!carnegieUndergradRequested && availableResultFieldCombined['carnegie_undergrad']) {
        fields.push(availableResultFieldCombined['carnegie_undergrad'])
    }

    const qbEverything = db
        .with("core_results", qbStart => {
            const qb0 = qbStart.selectFrom('search_base.dataset_merged as core')
                .select('institution_name')
                .select('unit_id')
                .select('ope6_id')
                .select('ope8_id')
                .select('data')
                .select('name_search_vector')
                .select('name_location_search_vector')

            let qqqbbb = qb0
            for (const field of fields) {
                const reference = resolveReference(field, availableResultFieldCombined, personalization)
                if (reference) {
                    qqqbbb = qqqbbb.select(sql.raw(reference).as(field.name))
                }
            }

            return qqqbbb
        })
        .selectFrom('core_results')
        .selectAll('core_results')
        //  always exclude unclassified and graduate only institutions
        //  they shouldn't be in the database but filter just in case
        //  NOT cannot be GINified; TODO: make it a positive filter using the known types
        .where(sql.raw("carnegie_undergrad"), "not in", sql.raw("('0', '-2')"))

    let qbUgc = qbEverything
    if (unitIds && unitIds.length > 0) {
        qbUgc = qbUgc.where('core_results.unit_id', 'in', unitIds)
    }

    return qbUgc
}

export const constructSearchQuery = async (
    availableResultFieldCombined: Record<string, ResultField>,
    input: SearchInput,
    personalization: SearchPersonalization,
    facetBuildersByName: Map<string, FacetBuilder>,
    page: number,
    limit: number,
    facetInputs: FacetInputValue[],
    requestedColumnNames: Set<string>,
    impliedColumns: Set<string>,
) => {
    const allErrors: FacetValidationError[] = []

    const requestedFields = Object.values(availableResultFieldCombined)
        .filter(f => requestedColumnNames.has(f.name) || impliedColumns.has(f.name))

    let qbFacetator = constructUnfacetedSearchQuery({
        availableResultFieldCombined,
        requestedFields,
        personalization,
    })

    for (const facetInput of facetInputs) {
        const facetBuilder = facetBuildersByName.get(facetInput.name)
        if (!facetBuilder) {
            continue
        }

        const facetation = facetBuilder.apply(availableResultFieldCombined, personalization, facetInput)
        if ('error' in facetation) {
            const errors: FacetValidationError[] = facetation.error
            allErrors.push(...errors)
        } else {

            const facet = facetBuilder.get()

            qbFacetator = qbFacetator.where(eb => {
                type XXX = typeof eb extends ExpressionBuilder<infer DBDB, infer TB>
                    ? { d: DBDB, t: TB }
                    : never
                const recur = (clause: Clause): ExpressionWrapper<XXX['d'], XXX['t'], SqlBool> => {
                    if (clause.type == 'json-contains') {
                        return eb(sql.raw('data'), '@>', sql.lit(JSON.stringify(clause.matchObject)))
                    }

                    if (clause.type == 'simple') {
                        const { left, operator, right } = clause
                        if (operator != '?|' && operator != '?&') {
                            const rod = sql.raw(left)
                            return eb.cmpr(rod, operator, right)
                        }
                        if (!Array.isArray(right)) {
                            throw new Error(`unexpected non-array value ${JSON.stringify(left)} for operator ${operator} on ${left} for facet ${facetInput.name}`)
                        }
                        if (right.length == 0) {
                            return eb.val(true)
                        }
                        const righted = Array.isArray(right) ? right.map(s => `'${s}'`) : [`'${right}'`]
                        return eb.cmpr(sql.raw(left), sql.raw(operator), sql.raw(`ARRAY[${righted.join(',')}]`))
                    }

                    if (clause.type == 'negative') {
                        return eb.not(recur(clause.clause))
                    }

                    if (clause.type == 'complex') {
                        //  TODO: smash together adjacent json-contains clauses
                        if (clause.operator == 'AND') {
                            return eb.and(clause.clauses.map(c => recur(c)))
                        }
                        if (clause.operator == 'OR') {
                            return eb.or(clause.clauses.map(c => recur(c)))
                        }
                        throw new Error(`unexpected clause operator ${clause.operator}`)
                    }
                    throw new Error(`unexpected clause ${JSON.stringify(clause)}`)
                }

                return recur(facetation.clause)
            })
        }
    }

    const quickSearch = input.quickSearch?.trim()
    if (quickSearch && quickSearch.length > 0) {
        if (quickSearch.match(/^[0-9]+$/)) {
            //  assume unit_id or ope6_id or ope6_id
            qbFacetator = qbFacetator.where(qb =>
                qb.or([
                    //  In theory these could be in the search index, but it's smaller without them,
                    //  and all we care about is exact match
                    qb('core_results.unit_id', '=', quickSearch),
                    qb('ope6_id', '=', quickSearch),
                    qb('ope8_id', '=', quickSearch),
                ])
            )
        } else {
            //  search against name
            qbFacetator = qbFacetator.where('name_location_search_vector', '@@', sql`plainto_tsquery('english', ${quickSearch})`)
        }
    }

    if (!input.sort?.length) {
        input.sort = [
            {
                field: 'name',
                direction: 'ASC',
            },
        ]
    }

    if (input.weights?.length) {
        const [sumExpression, scoreComponents] = constructWeightExpression(availableResultFieldCombined, input.weights, personalization)

        Object.entries(scoreComponents).forEach(([name, expression]) => {
            qbFacetator = qbFacetator.select(sql.raw(expression).as(`${name}_score`))
        })

        qbFacetator = qbFacetator.select(sql.raw(sumExpression).as('score'))
        qbFacetator = qbFacetator.orderBy(sql.raw('score'), 'desc')
    } else {
        const sort = input.sort ?? ['name']
        for (const s of sort) {
            if (!requestedColumnNames.has(s.field) && !impliedColumns.has(s.field)) {
                continue
            }
            const rf = availableResultFieldCombined[s.field]
            if (!rf) {
                logger.warn(`unsatisfied sort field '${s.field}'; ignoring`)
                continue
            }

            if (rf.sortable) {
                const reference = rf.name
                if (reference) {
                    qbFacetator = qbFacetator.orderBy(sql.raw(reference), s.direction == 'DESC' ? 'desc' : 'asc')
                }
            } else {
                logger.warn(`unsatisfied sort field '${s.field}'; ignoring`)
            }
        }
    }

    qbFacetator = qbFacetator.offset(page * limit)
        .limit(limit + 1)

    return { errors: allErrors, queryBuilder: qbFacetator }
}
