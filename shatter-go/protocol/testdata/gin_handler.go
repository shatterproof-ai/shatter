//go:build ignore

package testdata

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// ListUsers is a standard Gin handler.
func ListUsers(c *gin.Context) {
	c.JSON(http.StatusOK, []string{"alice", "bob"})
}

// CreateUser is a Gin handler with characteristic API usage.
func CreateUser(c *gin.Context) {
	name := c.Param("name")
	c.JSON(http.StatusCreated, map[string]string{"name": name})
}

// GinHelper is not a handler.
func GinHelper(s string) int {
	return len(s)
}
