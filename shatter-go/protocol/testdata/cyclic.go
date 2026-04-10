package testdata

// Cyclic struct types: A references B, B references A.
// Expected: analyzer resolves both without stack overflow,
// breaking the cycle with stub TypeInfo for the back-edge.

type NodeA struct {
	Name string
	Next *NodeB
}

type NodeB struct {
	Value int
	Back  *NodeA
}

// Self-referential struct: linked list node.
type ListNode struct {
	Val  int
	Next *ListNode
}

// ProcessCyclic accepts a cyclic struct parameter.
// Expected params: [{name: "a", type: {kind: "object", ...}}]
// The analyzer must not crash — cyclic fields get truncated.
func ProcessCyclic(a NodeA) string {
	if a.Next != nil {
		return "has next"
	}
	return "no next"
}

// ProcessSelfRef accepts a self-referential struct.
func ProcessSelfRef(node *ListNode) int {
	if node == nil {
		return 0
	}
	return node.Val
}
