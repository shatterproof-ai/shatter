package main

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// ListUsers is a standard Gin handler that returns a JSON array.
func ListUsers(c *gin.Context) {
	c.JSON(http.StatusOK, []string{"alice", "bob"})
}

// CreateUser is a Gin handler that reads a route parameter.
func CreateUser(c *gin.Context) {
	name := c.Param("name")
	c.JSON(http.StatusCreated, map[string]string{"name": name})
}

// GetStatus returns a plain text status response.
func GetStatus(c *gin.Context) {
	c.String(http.StatusOK, "ok")
}

// AbortExample demonstrates abort behavior.
func AbortExample(c *gin.Context) {
	auth := c.GetHeader("Authorization")
	if auth == "" {
		c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "unauthorized"})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "authorized"})
}

// NotAGinHandler is a regular function, not a handler.
func NotAGinHandler(s string) int {
	return len(s)
}
