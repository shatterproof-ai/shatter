import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { faker } from '@faker-js/faker';
import * as ts from 'typescript';
import { shatterAutotest } from "../core/shatter";
import { stringFakerses, optionVariantsMedium } from '../core/seed';
import { GeneratedParameter } from '../core/common';
namespace Nomen {
    export type Z = {
        a: number,
    };
}


describe('scratch space', () => {
    it('does', () => {
        const sourceCode = `
        type M = {
            mm: Map<number, any>,
        };

        type nono = number;

        namespace Nomen {
            export type Z = {
                a: number,
                bbbbbbbbbbbbbb?: string,
            };
        }
        
        interface MMM {
            nn: Map<number, any>,
        }

        function hello(m:Map<string, number>, m2:M, n:MMM, nz:Nomen.Z, mnmnmn:Map<string, Map<number, Map<boolean, any>>>, nooonoooo:nono) {
        }

        function zort(x:"a"|"b"|"c") {
        }

        const y = () => 0;
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'zert.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});

        const checker = program.getTypeChecker();

        const source = program.getSourceFile(sourceFilePath);


        const dumpType = (type: ts.Type, line: number) => {
            return `TYPE (${line}): type.aliasSymbol?.getName() = ${type.aliasSymbol?.getName()
                }; type.pattern?.getText() = ${type.pattern?.getText()
                }; type.flags = ${type.flags
                }; type.symbol.getName() = ${type.symbol?.getName()
                }; type to string = ${checker.typeToString(type)
                }; alias type args = ${type.aliasTypeArguments?.map((t) => checker.typeToString(t)).join(', ')})
                }`;
        };

        const visitor = (node: ts.Node) => {
            if (ts.isFunctionLike(node)) {
                node.parameters.forEach((param) => {
                    const typeNode = param.type;
                    if (!typeNode) {
                        return;
                    }

                    const dumpTypeNode = (typeNode: ts.TypeNode, soFar: string[] = []) => {
                        const baseType = checker.getTypeFromTypeNode(typeNode);
                        const line = ts.getLineAndCharacterOfPosition(source!, typeNode.pos).line;

                        console.log(`PARAMETER NODE Name: ${param.name.getText()} ${soFar.join('.')}; flags = ${typeNode.flags
                            }; text = ${typeNode.getText()
                            }; dumpTypes = ${dumpType(baseType, line)
                            }`);

                        if (ts.isTypeReferenceNode(typeNode)) {
                            const referencedType = checker.getTypeFromTypeNode(typeNode);
                            if (referencedType) { }
                            console.log(`TYPE REFERENCE NODE: ${typeNode.typeName.getText()}; typeNode.flags = ${typeNode.flags /*}; FQN = ${checker.getFullyQualifiedName(referencedType.symbol)
                            */}; dumpTypes = ${dumpType(referencedType, -1)
                                }`);
                            typeNode.typeArguments?.forEach((typeArg, i) => {
                                dumpTypeNode(typeArg, soFar.concat([`typeArg${i}`]));
                            });
                        }
                        if (ts.isUnionTypeNode(typeNode)) {
                            typeNode.types.forEach((type, i) => {
                                dumpTypeNode(type, soFar.concat([`type${i}`]));
                            });
                        }
                        if (ts.isStringLiteralLike(typeNode)) {
                            console.log(`STRING LITERAL NODE: ${typeNode.text}`);
                        }

                    };

                    dumpTypeNode(typeNode);
                });
                return;
            }
            if (ts.isInterfaceDeclaration(node)) {

            }
            if (ts.isTypeAliasDeclaration(node)) {
            }
            node.getChildren().forEach((child) => {
                ts.visitNode(child, visitor);
            });
            return node;
        };
        ts.visitNode(source, visitor);

    });
});


describe('scratch space 55', () => {
    it('does', () => {
        const sourceCode = `
type N = string & number;
type SS = {
k: number
} & {
j: string
};

type NN = {
k: number
} & {
k: string
};

type SSNN = {
n: N;
ss: SS;
nn: NN;
a: number|string;
b: boolean;
};

type SSSNNN = Pick<SSNN, "n"|"ss"|"nn"|"a">;

const s:SSSNNN = {} as any;
`;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'zert.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});

        const checker = program.getTypeChecker();

        const source = program.getSourceFile(sourceFilePath);

        const visitType = (type: ts.Type): string => {
            if (type.isIntersection()) {
                const pieces = type.types.flatMap(t => visitType(t));
                return pieces.join('&');
            } else {
                if (type.isLiteral()) {
                    return checker.typeToString(type);
                }

                const simpleTypeFlags = [
                    ts.TypeFlags.Any,
                    ts.TypeFlags.Unknown,
                    ts.TypeFlags.String,
                    ts.TypeFlags.Number,
                    ts.TypeFlags.Boolean,
                    ts.TypeFlags.StringLike,
                    ts.TypeFlags.NumberLike,
                    ts.TypeFlags.BooleanLike
                ];
                if (simpleTypeFlags.includes(type.flags)) {
                    return checker.typeToString(type);
                }

                return "{ " + checker.getPropertiesOfType(type).map((p) => {
                    if (p.valueDeclaration) {
                        const propt = checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration);
                        const t = visitType(propt);
                        return `${p.getName()}: ${t}`;
                    }
                    return "<novad>";
                }).join(", ") + " }";
            }

        };

        const visitor = (node: ts.Node) => {
            if (ts.isTypeNode(node)) {
                const start = ts.getLineAndCharacterOfPosition(source!, node.pos);
                const end = ts.getLineAndCharacterOfPosition(source!, node.end);
                if (ts.isTypeReferenceNode(node)) {
                    node.typeName.getText();
                    const t = checker.getTypeFromTypeNode(node);
                }
                const t = checker.getTypeFromTypeNode(node);
                const pos = `${start.line}:${start.character}-${end.line}:${end.character}`;
                console.log(`111 ${pos} Type = ${visitType(t)}`);
            }
            node.getChildren().forEach((child) => {
                ts.visitNode(child, visitor);
            });
            return node;
        };
        ts.visitNode(source, visitor);

    });
});

describe('scratch space 66', () => {
    it('does', () => {
        const sourceCode = `
type N = {
so?: number;
sr: number;
};
`;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'zert.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});

        const checker = program.getTypeChecker();

        const source = program.getSourceFile(sourceFilePath);

        const visitType = (type: ts.Type): string => {
            if (type.isIntersection()) {
                const pieces = type.types.flatMap(t => visitType(t));
                return pieces.join('&');
            } else {
                if (type.isLiteral()) {
                    return checker.typeToString(type);
                }

                const simpleTypeFlags = [
                    ts.TypeFlags.Any,
                    ts.TypeFlags.Unknown,
                    ts.TypeFlags.String,
                    ts.TypeFlags.Number,
                    ts.TypeFlags.Boolean,
                    ts.TypeFlags.StringLike,
                    ts.TypeFlags.NumberLike,
                    ts.TypeFlags.BooleanLike
                ];
                if (simpleTypeFlags.includes(type.flags)) {
                    return checker.typeToString(type);
                }

                return "{ " + checker.getPropertiesOfType(type).map((p) => {
                    if (p.valueDeclaration) {
                        const propt = checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration);

                        for (const d of p.getDeclarations() ?? []) {
                            if (ts.isPropertySignature(d)) {
                                const q = d.questionToken;
                                const r = q;
                            }
                            if (ts.isPropertyDeclaration(d)) {
                                const q = d.questionToken;
                                const r = q;
                            }
                        }

                        const isOptional = p.getDeclarations()?.find(d => ts.isPropertySignature(d) && d.questionToken);

                        const qts = p.getDeclarations()?.map((d) => ts.isQuestionToken(d));
                        const t = visitType(propt);
                        return `${p.getName()}${isOptional ? '?' : ''}: ${t}`;
                    }
                    return "<novad>";
                }).join(", ") + " }";
            }

        };

        const visitor = (node: ts.Node) => {
            if (ts.isTypeNode(node)) {
                const start = ts.getLineAndCharacterOfPosition(source!, node.pos);
                const end = ts.getLineAndCharacterOfPosition(source!, node.end);
                if (ts.isTypeReferenceNode(node)) {
                    node.typeName.getText();
                    const t = checker.getTypeFromTypeNode(node);
                }
                const t = checker.getTypeFromTypeNode(node);
                const pos = `${start.line}:${start.character}-${end.line}:${end.character}`;
                console.log(`111 ${pos} Type = ${visitType(t)}`);
            }
            node.getChildren().forEach((child) => {
                ts.visitNode(child, visitor);
            });
            return node;
        };
        ts.visitNode(source, visitor);

    });
});


describe('scratch space 26223423', () => {
    it('typeses', () => {
        const sourceCode = `
        type X = {
            x: number,
        }
        
        type Y<T> = {
            t: T,
        }
        
        type Z = Y<X>;
        type XX = Map<string, number>;
        type XX<Y, Z> = Map<Y, Z>;
        `;

        //  alias type arguments are on the LEFT
        //  type arguments are on the RIGHT

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'zertzert.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});
        const checker = program.getTypeChecker();

        const sourceFile = program.getSourceFile(sourceFilePath);
        if (!sourceFile) {
            throw new Error(`No source file ${sourceFilePath}`);
        }

        const traverseType = (type: ts.Type) => {
            const typeReference = type as ts.TypeReference;
            console.log(`Type reference ${checker.typeToString(typeReference.target)
            } with flags ${type.flags} and object flags ${(type as any).objectFlags
                } with type arguments ${typeReference.typeArguments?.map((t) => checker.typeToString(t)).join(', ')
                } and alias type arguments  ${typeReference.aliasTypeArguments?.map((t) => checker.typeToString(t)).join(', ')}`);
        };

        const transformer = (ctx: ts.TransformationContext) => (sourceFile: ts.SourceFile): ts.SourceFile => {
            const visitor = (node: ts.Node): ts.Node => {

                if (ts.isTypeNode(node)) {
                    console.log('-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=');
                    console.log(`Type node ${node.getText()}`);
                    traverseType(checker.getTypeFromTypeNode(node));
                    console.log('-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=');
                    return node;
                }

                if (ts.isTypeAliasDeclaration(node)) {
                    console.log('-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=');
                    console.log(`Type alias declaration ${node.getText()}`);
                    traverseType(checker.getTypeFromTypeNode(node.type));
                    node.typeParameters?.forEach((typeParameter) => {
                        console.log(`Type parameter ${typeParameter.getText()}`);
                        if (typeParameter.constraint) {
                            traverseType(checker.getTypeFromTypeNode(typeParameter.constraint));
                        }
                    });
                    console.log('-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=');
                    return node;
                }

                return ts.visitEachChild(node, visitor, ctx);
            };

            ts.visitNode(sourceFile, visitor);

            return sourceFile;
        };

        const transformed = ts.transform(sourceFile, [transformer]);
    });
});

describe('scratch space 2', () => {
    it('should find every embedded literal number or string', () => {
        const sourceCode = `
        const yes = "no no no";
        type M = {
            mm: 7777,
        };

        const calibrrasshis = {
            x: {
                y: {
                    z: 5915,
                },
                y2: {
                    z2: {
                        _: "faofaffo",
                    }
                }
            }
        };

        function hello(m:string, n:number, o:any) {
            if (m == "hellp") {
                return 2510;
            }
            if (n == 5142 || (n == 9719 && (m == "hello" || m == "goodbye"))) {
                return 1192;
            }
            const t = (aaa:'bbbbbbb') => "ccccccccccccccccccc";
            if (o?.a?.b?.c == "that") {
                return 2531;
            }
            return -1412;
        }

        const zort = (x:"a"|"b"|"c") => {
            return 1111;
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'zert.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});

        const sourceFile = program.getSourceFile(sourceFilePath);
        if (!sourceFile) {
            throw new Error(`No source file ${sourceFilePath}`);
        }

        const literals: any[] = [];
        const transformer = (ctx: ts.TransformationContext) => (sourceFile: ts.SourceFile): ts.SourceFile => {
            const visitor = (node: ts.Node): ts.Node => {
                if (ts.isStringLiteral(node)) {
                    literals.push(node.text);
                }
                if (ts.isNumericLiteral(node)) {
                    const asNumber = parseFloat(node.text);
                    literals.push(asNumber);
                }

                return ts.visitEachChild(node, visitor, ctx);
            };

            ts.visitNode(sourceFile, visitor);

            return sourceFile;
        };

        const transformed = ts.transform(sourceFile, [transformer]);

        console.log(`Literals: ${JSON.stringify(literals.sort(), null, 2)}`);
    });
});

describe('extension', () => {
    it('should pass', async () => {
        const sourceCode = `
        function hello(n:number, msgKey:string, messages:Map<string, string>, s_unused:Set<number>) {
            if (n <= 0) {
                throw new Error("n must be at least 1");
            }
        
            if (n % 1 != 0) {
                throw new Error("n must be an integer");
            }
        
            const pieces:string[] = []
            
            switch (n) {
                case 3: {
                    console.log("n is 3");
                    break;
                }
                case 6: {
                    console.log("n is 6");
                    break;
                }
                case 10: {
                    console.log("n is 10");
                    break;
                }
                case 15: {
                    console.log("n is 15");
                    break;
                }
                case 40:
                case 41: {
                    console.log("n is 40 or 41");
                    break;
                }
            }

            const msg = messages.get(msgKey) ?? "default message";
            for (let i = 0; i < n; i++) {
                if (i > 50) {
                    break;
                }
                pieces.push(msg)
            }
        
            if (n % 2 == 0) {
                return pieces.join(", ")
            }
            return pieces.join("; ")
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const testfile = join(tempdir, 'hello.ts');
        writeFileSync(testfile, sourceCode);

        const functionName = "hello";

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

        const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
        const maxIterations = 500;
        // const maxTime = 120_000;
        const maxTime = 10_000;
        const { executed, instrumented } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            // console.log(`Received clusters ${JSON.stringify(clusters, null, 2)}`);
        }, { shatterproofModuleOverride, maxIterations, maxTime });
        const unexecuted = instrumented.filter((i) => !executed.includes(i));
        console.log(`Executed     ${executed}`);
        console.log(`Instrumented ${instrumented}`);
        console.log(`Missed       ${unexecuted}`);
    });
});

describe('extensionensiondate ', () => {
    it('should should pass', async () => {
        const sourceCode = `
        function timeDifference(targetDate: Date, baseDate: Date): string {
            let difference = targetDate.getTime() - baseDate.getTime();
            const future = difference > 0;
        
            difference = Math.abs(difference);
        
            const minute = 1000 * 60;
            const hour = minute * 60;
            const day = hour * 24;
        
            const days = Math.floor(difference / day);
            difference -= days * day;
        
            const hours = Math.floor(difference / hour);
            difference -= hours * hour;
        
            const minutes = Math.floor(difference / minute);
        
            let result = '';
        
            if (days) {
                result += \`\${days} day\${days > 1 ? 's' : ''}\`;
            }
            if (hours) {
                if (result) {
                    result += ', ';
                }
                result += \`\${hours} hour\${hours > 1 ? 's' : ''}\`;
            }
            if (minutes) {
                if (result) {
                    result += ', ';
                }
                result += \`\${minutes} minute\${minutes > 1 ? 's' : ''}\`;
            }
        
            if (!result) {
                return "right now";
            }
        
            if (future) {
                return \`\${result} from now\`;
            }
            return \`\${result} ago\`;
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const testfile = join(tempdir, 'dadatata.ts');
        writeFileSync(testfile, sourceCode);

        const functionName = "timeDifference";

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

        const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
        const maxIterations = 500;
        // const maxTime = 120_000;
        const maxTime = 10_000;
        const { executed, instrumented, clusters } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            // console.log(`Received clusters ${ JSON.stringify(clusters, null, 2) }`);
        }, { shatterproofModuleOverride, maxIterations, maxTime });
        const unexecuted = instrumented.filter((i) => !executed.includes(i));
        console.log(`Executed     ${executed}`);
        console.log(`Instrumented ${instrumented}`);
        console.log(`Missed       ${unexecuted}`);

        const testCases: GeneratedParameter[][] = [];
        clusters.forEach((cluster) => {
            cluster.specimens.forEach((specimen) => {
                testCases.push(specimen.parameters);
            });
        });

        console.log(`Test cases: ${JSON.stringify(testCases)}`);
    });
});

describe('extensionensionddfdsf', () => {
    it('should should pass', async () => {
        const sourceCode = `
        class C { }

        class Regexpp { }

        class Date { }

        function romannumeral(n: number): string {
        if (typeof n != 'number' || n < 0) {
            throw new Error(\`Invalid input \${n}\`)
            }
        
            if (n == 0) {
                return ""
            }
        
            if (n > 3999) {
                throw new Error(\`Invalid input \${n}\`)
            }
        
            if (n >= 1000) {
                return 'M' + romannumeral(n - 1000)
            }
            if (n >= 900) {
                return 'CM' + romannumeral(n - 900)
            }
            if (n >= 500) {
                return 'D' + romannumeral(n - 500)
            }
            if (n >= 400) {
                return 'CD' + romannumeral(n - 400)
            }
            if (n >= 100) {
                return 'C' + romannumeral(n - 100)
            }
            if (n >= 90) {
                return 'XC' + romannumeral(n - 90)
            }
            if (n >= 50) {
                return 'L' + romannumeral(n - 50)
            }
            if (n >= 40) {
                return 'XL' + romannumeral(n - 40)
            }
            if (n >= 10) {
                return 'X' + romannumeral(n - 10)
            }
            if (n == 9) {
                return 'IX'
            }
            if (n >= 5) {
                return 'V' + romannumeral(n - 5)
            }
            if (n == 4) {
                return 'IV'
            }
        
            return 'I'.repeat(n)
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const testfile = join(tempdir, 'roro.ts');
        writeFileSync(testfile, sourceCode);

        const functionName = "romannumeral";

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

        const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
        const maxIterations = 500;
        // const maxTime = 120_000;
        const maxTime = 10_000;
        const { executed, instrumented, clusters } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            // console.log(`Received clusters ${JSON.stringify(clusters, null, 2)}`);
        }, { shatterproofModuleOverride, maxIterations, maxTime });
        const unexecuted = instrumented.filter((i) => !executed.includes(i));
        console.log(`Executed     ${executed}`);
        console.log(`Instrumented ${instrumented}`);
        console.log(`Missed       ${unexecuted}`);

        const testCases: string[] = [];
        clusters.forEach((cluster) => {
            cluster.results.forEach((result) => {
                testCases.push(result.serializedParameterValues);
            });
        });

        console.log(`Test cases: ${testCases}`);
    });
});

describe('infinitue', () => {

    it('ddddd', () => {

        type X = {
            fx: (x: number) => X,
        };
    });
});


describe('complicated', () => {
    it('should pass', async () => {

        //  TODO: duh
        const testfile = "/home/ketan/project/shatter/examples/typescript/src/query-creator.ts";
        // const testfile = "/home/ketan/project/shatter/examples/typescript/src/query-creator-short.ts";

        const functionName = "constructSearchQuery";

        const modulePaths = ["/home/ketan/project/shatter/examples/typescript/node_modules"];

        const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
        const maxIterations = 500;
        // const maxTime = 120_000;
        const maxTime = 10_000;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));

        const { executed, instrumented } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            // console.log(`Received clusters ${JSON.stringify(clusters, null, 2)}`);
        }, { shatterproofModuleOverride, maxIterations, maxTime, inBand: true });
        const unexecuted = instrumented.filter((i) => !executed.includes(i));
        console.log(`Executed     ${executed}`);
        console.log(`Instrumented ${instrumented}`);
        console.log(`Missed       ${unexecuted}`);
    });
});


const permute = function* (first: [string, (string | number)[]], rest: [string, (string | number)[]][], opts: any = {}): Generator<any, void, unknown> {
    const [optionKey, optionValues] = first;
    for (const value of optionValues) {
        const newOpts = {
            ...opts,
            [optionKey]: value,
        };
        if (rest.length === 0) {
            yield newOpts;
        } else {
            yield* permute(rest[0], rest.slice(1), newOpts);
        }
    }
};

describe('throwaway', () => {
    it('does', () => {
        const strangs: string[] = [];
        Object.entries(stringFakerses).forEach(([domain, generators]) => {
            generators.forEach((generator) => {
                for (let i = 0; i < 1; i++) {
                    const generatorOptionVariants: any = optionVariantsMedium[generator];

                    const fd = faker[domain as keyof typeof faker];
                    const f = fd[generator as keyof typeof fd] as any;

                    if (generatorOptionVariants) {
                        const all: [string, any[]][] = Object.entries(generatorOptionVariants);
                        const [first, ...rest] = all;
                        for (const opts of permute(first, rest)) {
                            const v = f(opts);
                            strangs.push(v);
                        }
                    } else {
                        const v = f(generatorOptionVariants);
                        strangs.push(v);
                    }
                }
            });
        });
        console.log(JSON.stringify(strangs));
    });
});