/* Counter builtin - A simple counter that can be incremented, decremented, reset, and queried.
   
   This builtin maintains a persistent counter value that can be manipulated with various operations.
   
   Usage:
     counter              # Display current count
     counter inc [n]      # Increment by n (default: 1)
     counter dec [n]      # Decrement by n (default: 1)
     counter set n        # Set counter to n
     counter reset        # Reset counter to 0
*/

/* Disable bash's malloc wrappers to use standard malloc/free */
#define DISABLE_MALLOC_WRAPPERS 1

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Include bash builtin headers */
#include "builtins.h"
#include "shell.h"
#include "common.h"
#include "input.h"


extern BASH_INPUT bash_input;


/* Global counter variable */
static long counter_value = 0;

/* Helper function to parse integer arguments */
static int
parse_number (char *str, long *result)
{
  char *endptr;
  long val;
  
  if (!str || !*str)
    return 0;
    
  val = strtol(str, &endptr, 10);
  
  if (*endptr != '\0')
    {
      builtin_error ("%s: numeric argument required", str);
      return 0;
    }
    
  *result = val;
  return 1;
}


char *current_jobu_line = (char *)NULL;
int current_jobu_line_index = 0;

static int jobu_get (void)
{
//   printf("jobu_get called\n");
  size_t line_len;
  unsigned char c;

  if (current_jobu_line == 0)   {
    
    printf("my prompt here>");
      /* Allocate memory for the string instead of using a string literal */
      const char *input_str = "echo hello && sleep 1";
      line_len = strlen(input_str);
      current_jobu_line = (char *)malloc(line_len + 2);  /* +2 for \n and \0 */
      
      if (current_jobu_line == 0){
          return (EOF);
      }
      
      strcpy(current_jobu_line, input_str); 

      current_jobu_line_index = 0;

      /* Now append newline */
      current_jobu_line[line_len++] = '\n';
      current_jobu_line[line_len] = '\0';
    }

  if (current_jobu_line[current_jobu_line_index] == 0)  {
      free (current_jobu_line);
      current_jobu_line = (char *)NULL;
      return (jobu_get());
    }
  else  {
      c = current_jobu_line[current_jobu_line_index++];
      return (c);
    }
}

static int jobu_unget (int c)
{
  if (current_jobu_line_index && current_jobu_line) {
      current_jobu_line[--current_jobu_line_index] = c;
  }
  return (c);
}


/* Main builtin function */
int
counter_builtin (WORD_LIST *list)
{
  char *operation;
  long value;
  
  /* If no arguments, just display the current count */
  if (list == 0)
    {
      printf("%ld\n", counter_value);
      fflush(stdout);
      return (EXECUTION_SUCCESS);
    }
  
  /* Get the operation */
  operation = list->word->word;
  
  /* Handle different operations */
  if (strcmp(operation, "setinput") == 0) {
      INPUT_STREAM location;

    /* Initialize location to a dummy value - we handle everything in jobu_get() */
    location.string = "";  /* Empty string to avoid NULL dereference */
    /* Use st_stdin since we're providing custom get/unget functions */
    init_yy_io (jobu_get, jobu_unget, st_stdin, "jobu stdin", location);

    printf("Input set to jobu\n");
        
  }  else if (strcmp(operation, "inc") == 0 || strcmp(operation, "increment") == 0)
    {
      /* Increment counter */
      value = 1;  /* Default increment */
      if (list->next)
        {
          if (!parse_number(list->next->word->word, &value))
            return (EXECUTION_FAILURE);
        }
      counter_value += value;
      printf("%ld\n", counter_value);
    }
  else if (strcmp(operation, "dec") == 0 || strcmp(operation, "decrement") == 0)
    {
      /* Decrement counter */
      value = 1;  /* Default decrement */
      if (list->next)
        {
          if (!parse_number(list->next->word->word, &value))
            return (EXECUTION_FAILURE);
        }
      counter_value -= value;
      printf("%ld\n", counter_value);
    }
  else if (strcmp(operation, "set") == 0)
    {
      /* Set counter to specific value */
      if (!list->next)
        {
          builtin_error("set: numeric argument required");
          return (EXECUTION_FAILURE);
        }
      if (!parse_number(list->next->word->word, &value))
        return (EXECUTION_FAILURE);
      counter_value = value;
      printf("%ld\n", counter_value);
    }
  else if (strcmp(operation, "reset") == 0)
    {
      /* Reset counter to 0 */
      counter_value = 0;
      printf("%ld\n", counter_value);
    }
  else if (strcmp(operation, "get") == 0)
    {
      /* Just display the value */
      printf("%ld\n", counter_value);
    }
  else
    {
      /* Try to parse as a direct set operation */
      if (!parse_number(operation, &value))
        {
          builtin_error("%s: invalid operation (use: inc, dec, set, reset, or get)", operation);
          return (EXECUTION_FAILURE);
        }
      counter_value = value;
      printf("%ld\n", counter_value);
    }
  
  fflush(stdout);
  return (EXECUTION_SUCCESS);
}

/* Called when the builtin is loaded */
int
counter_builtin_load (char *s)
{

    // bash_input.type = stream_type::st_string;
    // bash_input.name = "from   counter";
    // bash_input.location.string =  "echo hello && sleep 2";
//   with_input_from_string("echo oiuqwe \0 sleep 1 \n echo qwe  \n sleep 1", "from counter");
//   with_input_from_stdin();

  printf("Counter builtin loaded. Initializing counter to 0.\n");
  counter_value = 0;  /* Initialize counter on load */
  return (1);
}

/* Called when the builtin is unloaded */
void
counter_builtin_unload (char *s)
{
    printf("Counter builtin unloaded.\n");
  /* Nothing special to clean up */
}

/* Documentation strings */
char *counter_doc[] = {
  "Simple counter builtin.",
  "",
  "Maintains a persistent counter value that can be manipulated.",
  "",
  "Options:",
  "  (no args)        Display current counter value",
  "  inc [n]          Increment counter by n (default: 1)",
  "  dec [n]          Decrement counter by n (default: 1)",
  "  set n            Set counter to specific value n",
  "  reset            Reset counter to 0",
  "  get              Display current counter value",
  "",
  "Exit Status:",
  "Returns success unless an invalid option or argument is given.",
  (char *)NULL
};

/* Builtin structure */
struct builtin counter_struct = {
  "counter",           /* builtin name */
  counter_builtin,     /* function implementing the builtin */
  BUILTIN_ENABLED,     /* initial flags for builtin */
  counter_doc,         /* array of long documentation strings */
  "counter [inc|dec|set|reset|get] [n]",  /* usage synopsis */
  0                    /* reserved for internal use */
};
