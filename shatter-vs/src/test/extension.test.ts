import { mkdtempSync, open, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { faker } from '@faker-js/faker';
import * as ts from 'typescript';
import { shatterAutotest } from "../core/shatter";
import { stringFakerses, optionVariantsMedium } from '../core/seed';
import { GeneratedParameter, extractGeneratedParameterValue } from '../core/common';
import { asyncs, finished, started } from '../core/util';
import { isEnumType } from '../core/generator';
namespace Nomen {
    export type Z = {
        a: number,
    };
}

type ZZ = {
    aa: 5,
    aaa?: 5,
    aaaa: number,
};

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


describe('enum scratch space', () => {
    it('does', () => {
        const sourceCode = `
        enum X {
            A=1,
            B,
            C='see?',
        }

        function (x:X) {
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'numpty.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});

        const checker = program.getTypeChecker();

        const source = program.getSourceFile(sourceFilePath);

        const visitor = (node: ts.Node) => {
            if (ts.isFunctionLike(node)) {
                node.parameters.forEach((param) => {
                    const typeNode = param.type;
                    if (!typeNode) {
                        return;
                    }

                    const ptype = checker.getTypeFromTypeNode(typeNode);
                    if (isEnumType(ptype)) {
                        const enumDeclaration = ptype.symbol.valueDeclaration;
                        if (enumDeclaration && ts.isEnumDeclaration(enumDeclaration)) {
                            enumDeclaration.members.forEach((member) => {
                                if (ts.isEnumMember(member)) {
                                    ts.isEnumMember(member);
                                    console.log(`ENUM MEMBER: ${member.name.getText()}; value is maybe ${checker.getConstantValue(member)}`);
                                }
                            });
                        }
                        const memberses = Array.from(ptype.symbol.members?.entries() ?? []).map(([k, v]) => [k, v.getName()]);
                        console.log(JSON.stringify(memberses, null, 2));
                    }
                });
                return;
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

interface TestOptions {
    maxTime?: number,
    maxIterations?: number,
    inBand?: false,
};

async function testThisCode(functionName: string, sourceCode: string, options: TestOptions = { maxTime: 30_000, maxIterations: 1000, inBand: false }) {
    const tempdir = mkdtempSync(join(tmpdir(), `shatter-test-${functionName}`));
    const testfile = join(tempdir, 'index.ts');
    writeFileSync(testfile, sourceCode);
    return testThisFile(testfile, functionName, options, tempdir);
}

async function testThisFile(testfile: string, functionName: string, options: TestOptions, tempdir?: string) {
    const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

    const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
    const { executed, instrumented, clusters } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
        // console.log(`Received clusters ${JSON.stringify(clusters, null, 2)}`);
    }, { shatterproofModuleOverride, ...options });

    const edgeCases: GeneratedParameter[][] = [];
    const resolvedEdgeCases: any[][] = [];
    const seenSpecimens = new Set<string>();
    clusters.forEach((cluster) => {
        [...cluster.leasts, ...cluster.mosts].forEach((specimen) => {
            if (seenSpecimens.has(specimen.id)) {
                return;
            }
            seenSpecimens.add(specimen.id);
            const resolvedParameters: any[] = [];
            edgeCases.push(specimen.parameters);
            for (const p of specimen.parameters) {
                const resolved = extractGeneratedParameterValue(p);
                resolvedParameters.push(resolved);
            }
            resolvedEdgeCases.push(resolvedParameters);
        });
    });

    const unexecuted = instrumented.filter((i) => !executed.includes(i));
    console.log(`Executed     ${executed}`);
    console.log(`Instrumented ${instrumented}`);
    console.log(`Missed       ${unexecuted}`);
    // console.log(`${edgeCases.length} edge cases: ${JSON.stringify(edgeCases, null, 2)}`);
    console.log(`Resolved edge cases: ${JSON.stringify(resolvedEdgeCases)}`);
    if (asyncs.size > 0) {
        console.log(`open asyncs = ${Array.from(asyncs)}`);
        console.log(`started = ${Array.from(started)}`);
        console.log(`finished = ${Array.from(finished)}`);
    }
}

describe('extension', () => {
    it('should pass', async () => {
        const sourceCode = `
        class Numbererer {
            constructor(public n:number) { }
        }

        function hello(nn:Numbererer, msgKey:string, messages:Map<string, string>, ) {
            const n = nn.n;
            if (n <= 0) {
                throw new Error("n must be at least 1");
            }
        
            if (n % 1 != 0) {
                throw new Error("n must be an integer");
            }
        
            const pieces:string[] = []
            
            switch (n) {
                case 3: {
                    console.log(\`333 n is \${n}, msgKey = \${msgKey}\`);
                    break;
                }
                case 6: {
                    console.log(\`666 n is \${n}, msgKey = \${msgKey}\`);
                    break;
                }
                case 10: {
                    console.log(\`1010 n is \${n}, msgKey = \${msgKey}\`);
                    break;
                }
                case 15: {
                    console.log(\`1515 n is \${n}, msgKey = \${msgKey}\`);
                    break;
                }
                case 40:
                    case 41: {
                    console.log(\`444 n is \${n}, msgKey = \${msgKey}\`);
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

        await testThisCode("hello", sourceCode, { maxIterations: 2000, maxTime: 30_000 });
    });
});

describe('extinction ', () => {
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

        await testThisCode("timeDifference", sourceCode, { maxIterations: 500, maxTime: 10_000 });
    });
});

describe('distinction', () => {
    it('should should pass pass', async () => {
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

        await testThisCode("romannumeral", sourceCode, { maxIterations: 500, maxTime: 10_000 });
    });
});

/*
function getConstructorParameters(type: ts.Type, checker: ts.TypeChecker): ts.Symbol[] {
    const constructor = type.symbol.members?.get(ts.InternalSymbolName.Constructor);
    if (!constructor) {
        throw new Error(`No constructor for ${checker.typeToString(type)}`);
    }

    if (ts.isdeclara) {

    }

    const parameters = constructor.valueDeclaration?.parameters;
    if (!parameters) {
        throw new Error(`No parameters for ${checker.typeToString(type)}`);
    }
    return parameters.map((p) => {
        const symbol = checker.getSymbolAtLocation(p.name);
        if (!symbol) {
            throw new Error(`No symbol for ${p.name.getText()}`);
        }
        return symbol;
    });
}
*/
describe('class scratch fever', () => {

    class AAAA {
        public constructor() { }
    };

    const x = AAAA;
    new x();

    new AAAA();

    it('does does', async () => {
        const sourceCode = `

        // import ts = require('typescript');

        export namespace Nomen {
            export class X {
                public constructor(private n:number) {}
            }
        }

        export class Y {
            y = "hello";
        }

        class T {
            // constructor(n:ts.Node, nnx:Nomen.X) {}
            // constructor(nnx:Nomen.X) {}
            constructor() {}
            meth() {
                const x = "man";
            }
        }

        class Numbererer {
            constructor(public n:number) { }
        }

        function faff(n:Numbererer) {}

        `;

        /*
        1) compile the code above
        2) parse it
        3) extract the constructor
        4) write  to a file
        5) import it
        6) instantiate it
        */

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'classcratch.ts');
        writeFileSync(sourceFilePath, sourceCode);
        console.log(`path = ${sourceFilePath}`);

        const program = ts.createProgram([sourceFilePath], {});

        const checker = program.getTypeChecker();

        const source = program.getSourceFile(sourceFilePath);

        const fqns: string[] = [];
        const nems: string[] = [];

        let belowDecl = false;
        let depth = 0;
        const visitor = (node: ts.Node) => {
            if (ts.isConstructSignatureDeclaration(node)) {
                const sigdec = checker.getSignatureFromDeclaration(node);
                console.log(`Construct signature ${node.getText()} ${sigdec}`);
            }
            if (ts.isTypeReferenceNode(node)) {
                console.log(`parent = ${node.parent.getText()}`);
                const type = checker.getTypeAtLocation(node);
                console.log(`basic type ${checker.typeToString(type)} type flags = ${type.flags}`);

                const tttt = checker.getTypeOfSymbol(type.symbol);
                const constructors = checker.getSignaturesOfType(tttt, ts.SignatureKind.Construct);
                const callables = checker.getSignaturesOfType(tttt, ts.SignatureKind.Call);

                console.log(`symbol ${type.symbol.getName()} type ${checker.typeToString(tttt)} flags = ${type.flags} has ${constructors.length} constructors`);
                if (constructors.length > 0) {
                    const c = constructors[0];
                    const params = c.getParameters().map((p) => [checker.typeToString(checker.getTypeOfSymbol(p)), p.getName()]);
                    console.log(`params = ${JSON.stringify(params)}`);
                }
            }

            if (ts.isClassDeclaration(node)) {
                belowDecl = true;
                const type = checker.getTypeAtLocation(node);
                if (!type.isClass()) {
                    throw new Error(`Expected class in declaration but got ${checker.typeToString(type)}`);
                }

                const descls = type.symbol.getDeclarations();
                const cd = descls?.find((d) => (d.kind === ts.SyntaxKind.ClassDeclaration || d.kind === ts.SyntaxKind.ClassExpression));
                if (cd) {
                    const cdt = checker.getTypeAtLocation(cd);
                    // console.log(`Class declaration ${(type as any).id} ${(cdt as any).id} ${cd.getText()} = ${checker.typeToString(cdt)}`);
                }

                const ttttt = checker.getTypeOfSymbolAtLocation(type.symbol, node);

                const fqn = checker.getFullyQualifiedName(type.symbol);
                fqns.push(fqn);
                nems.push(type.symbol.getName());

                // console.log(`Class ${type.symbol.getName()} => ${checker.getPropertiesOfType(type).map(s => s.getName()).join(", ")}`);

                // const constructors = type.getConstructSignatures();
                const constructors = checker.getSignaturesOfType(type, ts.SignatureKind.Construct);
                const callables = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
                if (constructors.length === 0) {
                    // console.log(`No constructors at depth ${depth} for ${node.getText()} with ${node.getChildCount()} children`);
                } else {
                    console.log(`${constructors.length} for ${node.getText()}`);
                }

                depth++;
                node.getChildren().forEach((child) => {
                    ts.visitNode(child, visitor);
                });
                depth--;
                belowDecl = false;
            } else {
                if (belowDecl) {
                    // console.log(`Child at depth ${depth} is ${ts.SyntaxKind[node.kind]}`);
                }

                if (ts.isFunctionDeclaration(node)) {
                    node.parameters.forEach((p) => {
                        const ptype = checker.getTypeAtLocation(p);
                        const pctors = checker.getSignaturesOfType(ptype, ts.SignatureKind.Construct);
                        console.log(`ptype = ${checker.typeToString(ptype)} => ${pctors.length} constructors`);
                    });
                }

                depth++;
                node.getChildren().forEach((child) => {
                    ts.visitNode(child, visitor);
                });
                depth--;
            }
            return node;
        };
        ts.visitNode(source, visitor);

        console.log(`fqns = ${JSON.stringify(fqns)}`);
        console.log(`nems = ${JSON.stringify(nems)}`);

        await import(sourceFilePath).then((m) => {
            console.log(`m keys = ${Object.keys(m)}`);
            const xclass = m['Nomen']['X'];
            // console.log(`xclass = ${xclass}`);
            const x = new xclass(5);

            const yclass = m['Y'];
            // console.log(`yclass = ${yclass}`);
            const y = new yclass();

            console.log(`x = ${x}, x.n = ${x.n}`);
            console.log(`y = ${y}, y.y = ${y.y}`);
        });

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
        await testThisFile(testfile, functionName, { maxIterations: 500, maxTime: 10_000 });
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